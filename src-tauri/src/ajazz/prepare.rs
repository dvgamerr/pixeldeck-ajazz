use super::MANAGED_DEVICES;

use ajazz_sdk::{Kind, asynchronous::AsyncAjazz};

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
pub(super) async fn prepare_device(device: &AsyncAjazz, device_id: &str) -> bool {
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
