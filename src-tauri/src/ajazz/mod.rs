mod events;
mod images;
mod prepare;

pub use images::update_images;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use ajazz_sdk::asynchronous::AsyncAjazz;
use dashmap::DashMap;
use events::run_device_loop;
use once_cell::sync::Lazy;
use prepare::prepare_device;
use tokio::sync::RwLock;

static AJAZZ_DEVICES: Lazy<RwLock<HashMap<String, AsyncAjazz>>> = Lazy::new(|| RwLock::new(HashMap::new()));
static MANAGED_DEVICES: Lazy<RwLock<HashSet<String>>> = Lazy::new(|| RwLock::new(HashSet::new()));
static PROFILE_RENDERING_GATES: Lazy<DashMap<String, Arc<RwLock<bool>>>> = Lazy::new(DashMap::new);

fn profile_rendering_gate(id: &str) -> Arc<RwLock<bool>> {
	PROFILE_RENDERING_GATES.entry(id.to_owned()).or_insert_with(|| Arc::new(RwLock::new(false))).clone()
}

pub async fn resume_profile_rendering(id: &str) {
	*profile_rendering_gate(id).write().await = false;
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
