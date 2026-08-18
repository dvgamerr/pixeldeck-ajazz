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
mod platform {
	use super::AudioSnapshot;

	use anyhow::{Context, Result, bail};
	use com_policy_config::{IPolicyConfig, PolicyConfigClient};
	use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
	use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
	use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioMeterInformation};
	use windows::Win32::Media::Audio::{
		DEVICE_STATE_ACTIVE, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, eCommunications,
		eConsole, eMultimedia, eRender,
	};
	use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToString};
	use windows::Win32::System::Com::{
		CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
		CoUninitialize, STGM_READ,
	};
	use windows::core::{HSTRING, PCWSTR, PWSTR};

	struct ComApartment {
		uninitialize: bool,
	}

	impl ComApartment {
		fn enter() -> Result<Self> {
			// SAFETY: Every call in this module runs synchronously on the current thread.
			let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
			match result {
				status if status.is_ok() => Ok(Self { uninitialize: true }),
				RPC_E_CHANGED_MODE => Ok(Self {
					uninitialize: false,
				}),
				status => Err(windows::core::Error::from_hresult(status))
					.context("failed to initialise COM for audio control"),
			}
		}
	}

	impl Drop for ComApartment {
		fn drop(&mut self) {
			if self.uninitialize {
				// SAFETY: This balances the successful CoInitializeEx call on this thread.
				unsafe { CoUninitialize() };
			}
		}
	}

	struct Endpoint {
		id: String,
		name: String,
		device: IMMDevice,
	}

	fn take_pwstr(value: PWSTR) -> Result<String> {
		// SAFETY: GetId returns a null-terminated string allocated with CoTaskMemAlloc.
		let result = unsafe { value.to_string() };
		// SAFETY: This frees the buffer returned by IMMDevice::GetId exactly once.
		unsafe { CoTaskMemFree(Some(value.0.cast())) };
		result.context("audio endpoint ID was not valid UTF-16")
	}

	fn friendly_name(device: &IMMDevice) -> Result<String> {
		// SAFETY: The endpoint is active for the lifetime of the returned property store.
		let store = unsafe { device.OpenPropertyStore(STGM_READ) }
			.context("failed to open audio endpoint properties")?;
		// SAFETY: PKEY_Device_FriendlyName is a valid property key.
		let mut value = unsafe { store.GetValue(&PKEY_Device_FriendlyName) }
			.context("failed to read audio endpoint name")?;
		let mut buffer = [0u16; 512];
		// SAFETY: The output slice is writable and the PROPVARIANT remains alive for the call.
		let converted = unsafe { PropVariantToString(&value, &mut buffer) };
		// SAFETY: This clears the PROPVARIANT returned by IPropertyStore::GetValue.
		let cleared = unsafe { PropVariantClear(&mut value) };
		converted.context("failed to convert audio endpoint name")?;
		cleared.context("failed to clear audio endpoint property")?;
		let end = buffer
			.iter()
			.position(|character| *character == 0)
			.unwrap_or(buffer.len());
		Ok(String::from_utf16_lossy(&buffer[..end]))
	}

	fn enumerator() -> Result<IMMDeviceEnumerator> {
		// SAFETY: COM has been initialised on the current thread by the caller.
		unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
			.context("failed to create the Windows audio device enumerator")
	}

	fn endpoint_snapshot(device: &IMMDevice, name: String) -> Result<AudioSnapshot> {
		// SAFETY: IAudioEndpointVolume is supported by active render endpoints.
		let volume: IAudioEndpointVolume = unsafe { device.Activate(CLSCTX_ALL, None) }
			.context("failed to open the endpoint volume control")?;
		// SAFETY: The COM interface remains alive for both calls.
		let scalar = unsafe { volume.GetMasterVolumeLevelScalar() }
			.context("failed to read the system volume")?;
		// SAFETY: The COM interface remains alive for the call.
		let muted = unsafe { volume.GetMute() }
			.context("failed to read the system mute state")?
			.as_bool();
		// SAFETY: IAudioMeterInformation is supported by active render endpoints.
		let meter: IAudioMeterInformation = unsafe { device.Activate(CLSCTX_ALL, None) }
			.context("failed to open the endpoint audio meter")?;
		// SAFETY: The audio meter remains alive for this synchronous call.
		let peak =
			unsafe { meter.GetPeakValue() }.context("failed to read the output audio peak")?;
		Ok(AudioSnapshot {
			device_name: name,
			volume: (scalar.clamp(0.0, 1.0) * 100.0).round() as u8,
			muted,
			peak: (peak.clamp(0.0, 1.0) * 100.0).round() as u8,
		})
	}

	fn default_endpoint(enumerator: &IMMDeviceEnumerator) -> Result<Endpoint> {
		// SAFETY: The enumerator is valid and eRender/eMultimedia identify the default output.
		let device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia) }
			.context("no default sound output device is available")?;
		// SAFETY: IMMDevice::GetId returns a CoTaskMem-allocated string.
		let id = take_pwstr(unsafe { device.GetId() }?)?;
		let name = friendly_name(&device)?;
		Ok(Endpoint { id, name, device })
	}

	fn active_endpoints(enumerator: &IMMDeviceEnumerator) -> Result<Vec<Endpoint>> {
		// SAFETY: The enumerator is valid and the requested state/dataflow are supported.
		let collection = unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) }
			.context("failed to list active sound output devices")?;
		// SAFETY: The collection remains alive while it is enumerated.
		let count = unsafe { collection.GetCount() }?;
		let mut endpoints = Vec::with_capacity(count as usize);
		for index in 0..count {
			// SAFETY: index is within the count returned by this collection.
			let device = unsafe { collection.Item(index) }?;
			// SAFETY: IMMDevice::GetId returns a CoTaskMem-allocated string.
			let id = take_pwstr(unsafe { device.GetId() }?)?;
			let name = friendly_name(&device)?;
			endpoints.push(Endpoint { id, name, device });
		}
		endpoints.sort_by_key(|endpoint| endpoint.name.to_lowercase());
		Ok(endpoints)
	}

	fn with_default<T>(action: impl FnOnce(Endpoint) -> Result<T>) -> Result<T> {
		let _apartment = ComApartment::enter()?;
		action(default_endpoint(&enumerator()?)?)
	}

	pub fn snapshot() -> Result<AudioSnapshot> {
		with_default(|endpoint| endpoint_snapshot(&endpoint.device, endpoint.name))
	}

	pub fn change_volume(delta: i32) -> Result<AudioSnapshot> {
		with_default(|endpoint| {
			// SAFETY: IAudioEndpointVolume is supported by active render endpoints.
			let volume: IAudioEndpointVolume =
				unsafe { endpoint.device.Activate(CLSCTX_ALL, None) }
					.context("failed to open the endpoint volume control")?;
			// SAFETY: The COM interface remains alive for all calls.
			let current = unsafe { volume.GetMasterVolumeLevelScalar() }?;
			let next = (current + delta as f32 / 100.0).clamp(0.0, 1.0);
			// SAFETY: A null event context is explicitly supported by the Core Audio API.
			unsafe {
				volume.SetMasterVolumeLevelScalar(next, std::ptr::null())?;
				if delta != 0 {
					volume.SetMute(false, std::ptr::null())?;
				}
			}
			endpoint_snapshot(&endpoint.device, endpoint.name)
		})
	}

	pub fn switch_device(direction: i32) -> Result<AudioSnapshot> {
		let _apartment = ComApartment::enter()?;
		let enumerator = enumerator()?;
		let current = default_endpoint(&enumerator)?;
		let endpoints = active_endpoints(&enumerator)?;
		if endpoints.len() < 2 {
			bail!("at least two active sound output devices are required");
		}
		let current_index = endpoints
			.iter()
			.position(|endpoint| endpoint.id == current.id)
			.unwrap_or(0);
		let target_index =
			(current_index as i32 + direction).rem_euclid(endpoints.len() as i32) as usize;
		let target = &endpoints[target_index];
		let target_id = HSTRING::from(&target.id);
		let target_id = PCWSTR(target_id.as_ptr());
		// SAFETY: COM is initialised, PolicyConfigClient is the registered policy COM class,
		// and the endpoint ID stays alive for all three role updates.
		let policy: IPolicyConfig =
			unsafe { CoCreateInstance(&PolicyConfigClient, None, CLSCTX_ALL) }
				.context("failed to open the Windows sound device policy")?;
		// Set every role so applications do not keep using a stale communications endpoint.
		unsafe {
			policy.SetDefaultEndpoint(target_id, eConsole)?;
			policy.SetDefaultEndpoint(target_id, eMultimedia)?;
			policy.SetDefaultEndpoint(target_id, eCommunications)?;
		}
		endpoint_snapshot(&target.device, target.name.clone())
	}
}

#[cfg(not(windows))]
mod platform {
	use super::AudioSnapshot;
	use anyhow::{Result, bail};

	fn unsupported<T>() -> Result<T> {
		bail!("system audio actions are currently available on Windows")
	}

	pub fn snapshot() -> Result<AudioSnapshot> {
		unsupported()
	}

	pub fn change_volume(_delta: i32) -> Result<AudioSnapshot> {
		unsupported()
	}

	pub fn switch_device(_direction: i32) -> Result<AudioSnapshot> {
		unsupported()
	}
}

#[cfg(test)]
mod tests {
	use super::{
		ANIMATION_INTERVAL, AudioSnapshot, centered_status_layout, device_switch_indicator,
		driver_name, snapshot_image, text_width,
	};

	#[test]
	fn audio_animation_is_rate_limited_for_hid_displays() {
		assert_eq!(ANIMATION_INTERVAL, std::time::Duration::from_secs(1));
	}

	#[test]
	fn speaker_and_status_are_centered_as_one_group() {
		for status in ["0%", "50%", "100%", "MUTED"] {
			let (speaker_offset, status_center) = centered_status_layout(status);
			let speaker_left = 10.0 + speaker_offset;
			let status_right = status_center + text_width(status, 29) / 2.0;
			assert!(((speaker_left + status_right) / 2.0 - 88.0).abs() < 0.01);
		}
	}

	#[test]
	fn device_switch_uses_directional_chevrons_instead_of_text() {
		assert!(device_switch_indicator("PREV").contains("M14 95"));
		assert!(device_switch_indicator("NEXT").contains("M162 95"));
		assert!(!device_switch_indicator("PREV").contains("PREV"));
		assert!(!device_switch_indicator("NEXT").contains("NEXT"));
	}

	#[test]
	fn lcd_image_uses_native_zone_dimensions_and_font_paths() {
		let image = snapshot_image(
			&AudioSnapshot {
				device_name: "Speakers (Realtek Audio)".to_owned(),
				volume: 67,
				muted: false,
				peak: 60,
			},
			"",
		);

		assert!(image.starts_with("data:image/svg+xml,"));
		assert!(image.contains("width%3D%22176%22"));
		assert!(image.contains("height%3D%22112%22"));
		assert!(image.contains("translate%280%20-13%29"));
		assert!(image.contains("transform%3D%22translate"));
		assert!(!image.contains("%3Ctext"));
		assert!(image.len() < 50_000);
	}

	#[test]
	fn output_type_is_removed_from_device_name() {
		assert_eq!(
			driver_name("Speakers (Realtek(R) Audio)"),
			"Realtek(R) Audio"
		);
		assert_eq!(driver_name("Headphones - WH-1000XM5"), "WH-1000XM5");
		assert_eq!(
			driver_name("NVIDIA High Definition Audio"),
			"NVIDIA High Definition Audio"
		);
		assert_eq!(driver_name("Speakerphone USB"), "Speakerphone USB");
	}

	#[test]
	fn lcd_image_marks_muted_and_device_switch_states() {
		let muted = snapshot_image(
			&AudioSnapshot {
				device_name: "Output".to_owned(),
				volume: 50,
				muted: true,
				peak: 80,
			},
			"",
		);
		let switched = snapshot_image(
			&AudioSnapshot {
				device_name: "Output".to_owned(),
				volume: 50,
				muted: false,
				peak: 80,
			},
			"NEXT",
		);

		assert!(muted.contains("%23ff3155"));
		assert!(switched.contains("%23facc15"));
		assert!(!switched.contains("NEXT"));
		assert!(!switched.contains("PREV"));
		assert!(switched.contains("%23000"));
		assert!(!switched.contains("linearGradient"));
		assert!(!switched.contains("rx%3D"));
	}

	#[test]
	fn real_audio_peak_changes_the_pixel_speaker_frame() {
		let quiet = snapshot_image(
			&AudioSnapshot {
				device_name: "Output".to_owned(),
				volume: 50,
				muted: false,
				peak: 0,
			},
			"",
		);
		let loud = snapshot_image(
			&AudioSnapshot {
				device_name: "Output".to_owned(),
				volume: 50,
				muted: false,
				peak: 90,
			},
			"",
		);

		assert_ne!(quiet, loud);
		assert!(!quiet.contains("M52%2032"));
		assert!(loud.contains("M52%2032"));
	}

	#[test]
	fn audio_peak_animation_keeps_the_speaker_anchor_fixed() {
		let (speaker_offset, _) = centered_status_layout("50%");
		let expected_transform =
			format!("%3Cg%20transform%3D%22translate%28{speaker_offset:.2}%200%29%22%3E");

		for peak in [0, 10, 30, 90] {
			let image = snapshot_image(
				&AudioSnapshot {
					device_name: "Output".to_owned(),
					volume: 50,
					muted: false,
					peak,
				},
				"",
			);

			assert!(
				image.contains(&expected_transform),
				"speaker moved at peak {peak}"
			);
		}
	}

	#[cfg(windows)]
	#[test]
	fn windows_default_output_snapshot_is_readable() {
		let snapshot =
			super::platform::snapshot().expect("default Windows output should be readable");

		assert!(!snapshot.device_name.trim().is_empty());
		assert!(snapshot.volume <= 100);
	}
}
