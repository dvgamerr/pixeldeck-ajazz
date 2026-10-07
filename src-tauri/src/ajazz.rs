use crate::events::outbound::{encoder, keypad};

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use ajazz_sdk::{
	AjazzError, Event, Kind,
	asynchronous::{AsyncAjazz, DeviceImageUpdate},
};
use base64::Engine as _;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use tokio::sync::RwLock;
use tokio::time::Instant;

static AJAZZ_DEVICES: Lazy<RwLock<HashMap<String, AsyncAjazz>>> = Lazy::new(|| RwLock::new(HashMap::new()));
static MANAGED_DEVICES: Lazy<RwLock<HashSet<String>>> = Lazy::new(|| RwLock::new(HashSet::new()));
static PROFILE_RENDERING_GATES: Lazy<DashMap<String, Arc<RwLock<bool>>>> = Lazy::new(DashMap::new);
const INPUT_READ_TIMEOUT: Duration = Duration::from_millis(50);
const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(10);

fn profile_rendering_gate(id: &str) -> Arc<RwLock<bool>> {
	PROFILE_RENDERING_GATES.entry(id.to_owned()).or_insert_with(|| Arc::new(RwLock::new(false))).clone()
}

pub async fn resume_profile_rendering(id: &str) {
	*profile_rendering_gate(id).write().await = false;
}

pub async fn update_images(updates: Vec<(crate::shared::Context, Option<String>)>) -> Result<(), anyhow::Error> {
	let Some(device_id) = updates.first().map(|(context, _)| context.device.clone()) else {
		return Ok(());
	};
	if updates.iter().any(|(context, _)| context.device != device_id) {
		return Err(anyhow::anyhow!("An image batch cannot target multiple devices"));
	}

	let rendering_gate = profile_rendering_gate(&device_id);
	let rendering_paused = rendering_gate.read().await;
	if *rendering_paused {
		return Ok(());
	}

	let device = AJAZZ_DEVICES.read().await.get(&device_id).cloned();
	let Some(device) = device else {
		return Ok(());
	};

	let updates = tokio::task::block_in_place(move || {
		updates
			.into_iter()
			.map(|(context, image)| {
				let image = image
					.map(|image| {
						let (_, data) = image.split_once(',').ok_or_else(|| anyhow::anyhow!("Invalid image data URL"))?;
						let bytes = base64::engine::general_purpose::STANDARD.decode(data)?;
						Ok::<_, anyhow::Error>(image::load_from_memory(&bytes)?)
					})
					.transpose()?;
				Ok(if context.controller == "Encoder" {
					DeviceImageUpdate::TouchZone { touch: context.position, image }
				} else {
					DeviceImageUpdate::Button { key: context.position, image }
				})
			})
			.collect::<Result<Vec<_>, anyhow::Error>>()
	})?;

	device.apply_image_batch(updates).await?;
	Ok(())
}

pub async fn clear_screen(id: &str) -> Result<(), anyhow::Error> {
	let rendering_gate = profile_rendering_gate(id);
	let rendering_paused = rendering_gate.read().await;
	if *rendering_paused {
		return Ok(());
	}

	if let Some(device) = AJAZZ_DEVICES.read().await.get(id) {
		device.clear_all_button_images().await?;
		device.flush().await?;
	}
	Ok(())
}

pub async fn set_startup_image(id: &str, image: image::DynamicImage) -> Result<(), anyhow::Error> {
	let rendering_gate = profile_rendering_gate(id);
	let mut rendering_paused = rendering_gate.write().await;
	let was_paused = *rendering_paused;
	*rendering_paused = true;

	let devices = AJAZZ_DEVICES.read().await;
	let result = match devices.get(id) {
		Some(device) => device.set_logo_image(image).await.map_err(Into::into),
		None => Err(anyhow::anyhow!("Device is no longer connected")),
	};
	if result.is_err() && !was_paused {
		*rendering_paused = false;
	}
	result
}

pub async fn set_brightness(brightness: u8) {
	for device in AJAZZ_DEVICES.read().await.values() {
		let _ = device.set_brightness(brightness.clamp(0, 100)).await;
		let _ = device.flush().await;
	}
}

pub async fn reset_devices() {
	for device in AJAZZ_DEVICES.read().await.values() {
		let _ = device.reset().await;
		let _ = device.flush().await;
	}
}

fn opendeck_device_type(kind: Kind) -> u8 {
	match kind {
		Kind::Akp153 | Kind::Akp153E | Kind::Akp153R => 2,
		Kind::Akp815 => 2,
		Kind::Akp03 | Kind::Akp03E | Kind::Akp03R => 2,
		Kind::Akp03RRev2 => 2,
		Kind::Akp05E552A => 7,
	}
}

/// Clears images, applies brightness, flushes and registers the device with OpenDeck.
/// Returns `false` (after releasing the managed slot) when initialisation fails.
async fn prepare_device(device: &AsyncAjazz, device_id: &str) -> bool {
	let kind = device.kind();
	let startup_image = kind.boot_logo_size().map(|(width, height)| crate::shared::ImageSize {
		width: width as u16,
		height: height as u16,
	});
	if let Err(error) = device.clear_all_button_images().await {
		log::warn!("Failed to initialise {device_id}: {error}");
		MANAGED_DEVICES.write().await.remove(device_id);
		return false;
	}
	if let Ok(settings) = crate::store::get_settings()
		&& let Err(error) = device.set_brightness(settings.value.brightness).await
	{
		log::warn!("Failed to set brightness for {device_id}: {error}");
	}
	if let Err(error) = device.flush().await {
		log::warn!("Failed to flush initial state for {device_id}: {error}");
		MANAGED_DEVICES.write().await.remove(device_id);
		return false;
	}
	if let Err(error) = crate::events::inbound::devices::register_device(
		"",
		crate::events::inbound::PayloadEvent {
			payload: crate::shared::DeviceInfo {
				id: device_id.to_owned(),
				plugin: String::new(),
				name: device.product_name.to_owned(),
				rows: kind.row_count(),
				columns: kind.column_count(),
				encoders: kind.encoder_count(),
				r#type: opendeck_device_type(kind),
				startup_image,
			},
		},
	)
	.await
	{
		log::warn!("Failed to register {device_id}: {error}");
		MANAGED_DEVICES.write().await.remove(device_id);
		return false;
	}
	true
}

async fn dispatch_device_event(device_id: &str, update: Event) {
	let result = match update {
		Event::ButtonDown(key) => keypad::key_down(device_id, key).await,
		Event::ButtonUp(key) => keypad::key_up(device_id, key).await,
		Event::EncoderTwist(dial, ticks) => encoder::dial_rotate(device_id, dial, ticks.into()).await,
		Event::EncoderDown(dial) => encoder::dial_press(device_id, "dialDown", dial).await,
		Event::EncoderUp(dial) => encoder::dial_press(device_id, "dialUp", dial).await,
	};
	if let Err(error) = result {
		log::warn!("Failed to process device event {update:?}: {error}");
	}
}

/// Reads input with a bounded timeout and sends a keep-alive every `KEEP_ALIVE_INTERVAL`
/// until the device fails.
async fn run_device_loop(device: &AsyncAjazz, reader: &ajazz_sdk::asynchronous::AsyncDeviceStateReader, device_id: &str) {
	let mut next_keep_alive = Instant::now();
	loop {
		if Instant::now() >= next_keep_alive {
			if let Err(error) = device.keep_alive().await {
				log::warn!("Keep-alive failed for {device_id}: {error}");
				break;
			}
			next_keep_alive = Instant::now() + KEEP_ALIVE_INTERVAL;
		}

		let updates = match reader.read_timeout(INPUT_READ_TIMEOUT).await {
			Ok(updates) => updates,
			Err(AjazzError::BadData) => {
				log::debug!("Ignored unsupported input packet from {device_id}");
				continue;
			}
			Err(error) => {
				log::warn!("Device reader stopped for {device_id}: {error}");
				break;
			}
		};
		for update in updates {
			dispatch_device_event(device_id, update).await;
		}
	}
}

async fn init(device: AsyncAjazz, device_id: String) {
	if AJAZZ_DEVICES.read().await.contains_key(&device_id) {
		MANAGED_DEVICES.write().await.remove(&device_id);
		return;
	}

	if !prepare_device(&device, &device_id).await {
		return;
	}

	let reader = device.get_reader();
	AJAZZ_DEVICES.write().await.insert(device_id.clone(), device.clone());
	log::info!("Registered {} as {}", device.product_name, device_id);
	run_device_loop(&device, &reader, &device_id).await;

	AJAZZ_DEVICES.write().await.remove(&device_id);
	MANAGED_DEVICES.write().await.remove(&device_id);
	if let Err(error) = crate::events::inbound::devices::deregister_device("", crate::events::inbound::PayloadEvent { payload: device_id.clone() }).await {
		log::warn!("Failed to deregister {device_id}: {error}");
	}
}

/// Attempt to initialise all connected devices.
pub async fn initialise_devices() {
	if let Ok(settings) = crate::store::get_settings() {
		if settings.value.disabledevices {
			crate::plugins::DEVICE_NAMESPACES
				.write()
				.await
				.insert("sd".to_owned(), "opendeck_alternative_ajazz_implementation".to_owned());
			return;
		} else {
			crate::plugins::DEVICE_NAMESPACES.write().await.remove("sd");
		}
	}

	// Iterate through detected Ajazz devices and attempt to register them.
	match ajazz_sdk::new_hidapi() {
		Ok(hid) => {
			for (kind, serial) in ajazz_sdk::list_devices(&hid) {
				let device_id = format!("sd-{serial}");
				if !MANAGED_DEVICES.write().await.insert(device_id.clone()) {
					continue;
				}
				match AsyncAjazz::connect(&hid, kind, &serial) {
					Ok(device) => {
						tokio::spawn(init(device, device_id));
					}
					Err(error) => {
						MANAGED_DEVICES.write().await.remove(&device_id);
						log::warn!("Failed to connect to Ajazz device: {error}");
					}
				}
			}
		}
		Err(error) => log::warn!("Failed to initialise hidapi: {error}"),
	}
}
