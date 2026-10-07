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
	let peak = unsafe { meter.GetPeakValue() }.context("failed to read the output audio peak")?;
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
		let volume: IAudioEndpointVolume = unsafe { endpoint.device.Activate(CLSCTX_ALL, None) }
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
	let policy: IPolicyConfig = unsafe { CoCreateInstance(&PolicyConfigClient, None, CLSCTX_ALL) }
		.context("failed to open the Windows sound device policy")?;
	// Set every role so applications do not keep using a stale communications endpoint.
	unsafe {
		policy.SetDefaultEndpoint(target_id, eConsole)?;
		policy.SetDefaultEndpoint(target_id, eMultimedia)?;
		policy.SetDefaultEndpoint(target_id, eCommunications)?;
	}
	endpoint_snapshot(&target.device, target.name.clone())
}
