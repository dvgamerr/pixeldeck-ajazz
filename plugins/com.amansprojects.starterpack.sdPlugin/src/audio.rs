use crate::compat::*;
use std::{
	collections::HashMap,
	sync::{
		Arc, LazyLock, Mutex,
		atomic::{AtomicBool, Ordering},
	},
	time::{Duration, Instant},
};

use crate::{
	live::{self, LiveAction},
	pixel::{data_uri, text_path, text_width},
};

pub const ACTION: &str = "com.amansprojects.starterpack.systemvolume";
const DOUBLE_PRESS: Duration = Duration::from_secs(1);
// The LCD image is transferred as several HID reports. One frame per second
// keeps the peak indicator useful without continuously saturating the device.
const ANIMATION_INTERVAL: Duration = Duration::from_secs(1);
static LAST_PRESSES: LazyLock<Mutex<HashMap<String, Instant>>> = LazyLock::new(Default::default);
static LIVE: LazyLock<Mutex<LiveAction>> = LazyLock::new(Default::default);

#[derive(Clone, Debug, PartialEq)]
struct AudioSnapshot {
	device_name: String,
	volume: u8,
	muted: bool,
	peak: u8,
}

impl AudioSnapshot {
	fn same_frame(&self, other: &Self) -> bool {
		self.device_name == other.device_name
			&& self.volume == other.volume
			&& self.muted == other.muted
			&& peak_level(self) == peak_level(other)
	}
}

fn setting_u8(settings: &SettingsValue, key: &str, fallback: u8) -> u8 {
	settings
		.as_object()
		.and_then(|settings| settings.get(key))
		.and_then(|value| value.as_u64())
		.map(|value| value.min(u8::MAX as u64) as u8)
		.unwrap_or(fallback)
}

fn driver_name(value: &str) -> &str {
	let value = value.trim();
	for prefix in ["Speakers", "Speaker", "Headphones", "Headphone"] {
		if value
			.get(..prefix.len())
			.is_some_and(|start| start.eq_ignore_ascii_case(prefix))
		{
			let rest = &value[prefix.len()..];
			if rest
				.chars()
				.next()
				.is_some_and(|c| c.is_whitespace() || "()[]-:".contains(c))
			{
				let name = rest.trim_matches(|c: char| c.is_whitespace() || "()[]-:".contains(c));
				if !name.is_empty() {
					return name;
				}
			}
		}
	}
	value
}

fn truncate_label(value: &str, max_chars: usize) -> String {
	let mut chars = value.chars();
	let mut result: String = chars.by_ref().take(max_chars).collect();
	if chars.next().is_some() {
		result.pop();
		result.push('…');
	}
	result
}

fn centered_status_layout(status: &str) -> (f32, f32) {
	const IMAGE_CENTER: f32 = 88.0;
	const SPEAKER_LEFT: f32 = 10.0;
	const SPEAKER_WIDTH: f32 = 41.0;
	const GAP: f32 = 10.0;

	let status_width = text_width(status, 29);
	let row_left = IMAGE_CENTER - (SPEAKER_WIDTH + GAP + status_width) / 2.0;
	let speaker_offset = row_left - SPEAKER_LEFT;
	let status_center = row_left + SPEAKER_WIDTH + GAP + status_width / 2.0;
	(speaker_offset, status_center)
}

fn device_switch_indicator(event: &str) -> &'static str {
	match event {
		"PREV" => r##"<path d="M14 95l-5 4 5 4" fill="none" stroke="#facc15" stroke-width="2"/>"##,
		"NEXT" => r##"<path d="M162 95l5 4-5 4" fill="none" stroke="#facc15" stroke-width="2"/>"##,
		_ => "",
	}
}

fn snapshot_image(snapshot: &AudioSnapshot, event: &str) -> String {
	let volume = snapshot.volume.min(100);
	let bar_width = u16::from(volume) * 140 / 100;
	let accent = if snapshot.muted { "#ff3155" } else { "#20e3ff" };
	let status = if snapshot.muted {
		"MUTED".to_owned()
	} else {
		format!("{volume}%")
	};
	let device_name = truncate_label(&driver_name(&snapshot.device_name).to_uppercase(), 22);
	let (speaker_offset, status_center) = centered_status_layout(&status);
	let status_path = text_path(&status, status_center, 62, 29, accent);
	let device_path = text_path(&device_name, 88.0, 102, 10, "#a7b0c0");
	let level = peak_level(snapshot);
	let speaker_waves = if snapshot.muted {
		r##"<path d="M34 42h5v5h5v5h-5v5h-5v-5h-5v-5h5z" fill="#ff3155"/>"##
	} else {
		match level {
			0 => "",
			1 => r##"<path d="M32 44h5v5h4v6h-4v5h-5v-5h4v-6h-4z" fill="#20e3ff"/>"##,
			2 => {
				r##"<path d="M32 42h5v5h4v10h-4v5h-5v-5h4V47h-4zM42 39h5v5h4v16h-4v5h-5v-5h4V44h-4z" fill="#20e3ff"/>"##
			}
			_ => {
				r##"<path d="M32 42h5v5h4v10h-4v5h-5v-5h4V47h-4zM42 37h5v5h4v20h-4v5h-5v-5h4V42h-4zM52 32h5v5h4v30h-4v5h-5v-5h4V37h-4z" fill="#20e3ff"/>"##
			}
		}
	};
	let switch_indicator = device_switch_indicator(event);
	let speaker_x = speaker_offset;

	let svg = format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" width="176" height="112" viewBox="0 0 176 112" shape-rendering="crispEdges">
<rect width="176" height="112" fill="#000"/>
<g transform="translate(0 -13)">
<g transform="translate({speaker_x:.2} 0)">
<path d="M10 45h9v14h-9zM19 41h5v22h-5zM24 36h5v32h-5z" fill="#f8fafc"/>
{speaker_waves}
</g>
{status_path}
<rect x="18" y="75" width="140" height="8" fill="#20242c"/>
<rect x="18" y="75" width="{bar_width}" height="8" fill="{accent}"/>
<path d="M31 75v8m14-8v8m14-8v8m14-8v8m14-8v8m14-8v8m14-8v8m14-8v8m14-8v8" stroke="#000" stroke-width="2"/>
{device_path}
{switch_indicator}
</g>
</svg>"##
	);

	data_uri(&svg)
}

fn peak_level(snapshot: &AudioSnapshot) -> u8 {
	if snapshot.muted || snapshot.peak < 2 {
		0
	} else if snapshot.peak < 20 {
		1
	} else if snapshot.peak < 55 {
		2
	} else {
		3
	}
}

async fn render_snapshot(
	context: String,
	snapshot: AudioSnapshot,
	event: &str,
	outbound: &mut OutboundEventManager,
) -> EventHandlerResult {
	outbound
		.set_image(context, Some(snapshot_image(&snapshot, event)), None)
		.await?;
	Ok(())
}

async fn report_error(
	context: String,
	error: anyhow::Error,
	outbound: &mut OutboundEventManager,
) -> EventHandlerResult {
	log::warn!("Audio action failed for {context}: {error:#}");
	outbound.show_alert(context).await?;
	Err(error)
}

async fn render_result(
	context: String,
	result: anyhow::Result<AudioSnapshot>,
	event: &str,
	outbound: &mut OutboundEventManager,
) -> EventHandlerResult {
	match result {
		Ok(snapshot) => render_snapshot(context, snapshot, event, outbound).await,
		Err(error) => report_error(context, error, outbound).await,
	}
}

pub async fn rotate_volume(
	event: DialRotateEvent,
	outbound: &mut OutboundEventManager,
) -> EventHandlerResult {
	let context = event.context.clone();
	let step = setting_u8(&event.payload.settings, "step", 2).clamp(1, 20);
	let delta = i32::from(event.payload.ticks) * i32::from(step);
	render_result(context, platform::change_volume(delta), "", outbound).await
}

pub async fn press_device(
	event: DialPressEvent,
	outbound: &mut OutboundEventManager,
) -> EventHandlerResult {
	let context = event.context.clone();
	let now = Instant::now();
	let double = {
		let mut presses = LAST_PRESSES.lock().unwrap();
		presses.retain(|_, time| now.duration_since(*time) <= DOUBLE_PRESS);
		let double = presses.remove(&context).is_some();
		if !double {
			presses.insert(context.clone(), now);
		}
		double
	};
	let direction = if double { -2 } else { 1 };
	let indicator = if double { "PREV" } else { "NEXT" };
	render_result(
		context,
		platform::switch_device(direction),
		indicator,
		outbound,
	)
	.await
}

pub async fn refresh(context: String, outbound: &mut OutboundEventManager) -> EventHandlerResult {
	render_result(context, platform::snapshot(), "", outbound).await
}

async fn animate(cancel: Arc<AtomicBool>, mut previous: AudioSnapshot) {
	let start = tokio::time::Instant::now() + ANIMATION_INTERVAL;
	let mut interval = tokio::time::interval_at(start, ANIMATION_INTERVAL);
	interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
	while !cancel.load(Ordering::Acquire) {
		interval.tick().await;
		let snapshot = match platform::snapshot() {
			Ok(snapshot) => snapshot,
			Err(error) => {
				log::debug!("Audio peak refresh failed: {error:#}");
				continue;
			}
		};
		if previous.same_frame(&snapshot) {
			continue;
		}
		let image = snapshot_image(&snapshot, "");
		previous = snapshot;
		if let Err(error) = live::broadcast(&LIVE, image).await {
			log::warn!("Audio animation image update failed: {error}");
			break;
		}
	}
}

pub async fn appear(context: String, outbound: &mut OutboundEventManager) -> EventHandlerResult {
	let initial = match platform::snapshot() {
		Ok(snapshot) => snapshot,
		Err(error) => return report_error(context, error, outbound).await,
	};
	render_snapshot(context.clone(), initial.clone(), "", outbound).await?;

	let mut live = LIVE.lock().unwrap();
	if live.subscribe(context) {
		let cancel = Arc::new(AtomicBool::new(false));
		live.start(tokio::spawn(animate(cancel.clone(), initial)), cancel);
	}
	Ok(())
}

pub fn disappear(context: &str) {
	LIVE.lock().unwrap().unsubscribe(context);
}

#[cfg(windows)]
#[path = "audio/platform_windows.rs"]
mod platform;

#[cfg(not(windows))]
#[path = "audio/platform_other.rs"]
mod platform;

#[cfg(test)]
mod tests;
