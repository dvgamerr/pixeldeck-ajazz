use super::{AJAZZ_DEVICES, profile_rendering_gate};

use ajazz_sdk::asynchronous::DeviceImageUpdate;
use base64::Engine as _;

fn decode_image(image: &str) -> Result<image::DynamicImage, anyhow::Error> {
	let (_, data) = image.split_once(',').ok_or_else(|| anyhow::anyhow!("Invalid image data URL"))?;
	let bytes = base64::engine::general_purpose::STANDARD.decode(data)?;
	Ok(image::load_from_memory(&bytes)?)
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
				let image = image.as_deref().map(decode_image).transpose()?;
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
