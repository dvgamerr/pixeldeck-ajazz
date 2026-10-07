use crate::events::outbound::{encoder, keypad};

use std::time::Duration;

use ajazz_sdk::{
	AjazzError, Event,
	asynchronous::{AsyncAjazz, AsyncDeviceStateReader},
};
use tokio::time::Instant;

const INPUT_READ_TIMEOUT: Duration = Duration::from_millis(50);
const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(10);

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
pub(super) async fn run_device_loop(device: &AsyncAjazz, reader: &AsyncDeviceStateReader, device_id: &str) {
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
