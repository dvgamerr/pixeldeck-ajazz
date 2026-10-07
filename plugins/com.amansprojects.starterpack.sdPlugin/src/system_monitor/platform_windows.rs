use super::MonitorSnapshot;
use std::{
	collections::HashMap,
	mem::MaybeUninit,
	os::windows::process::CommandExt,
	process::{Command, Stdio},
	sync::mpsc::{self, Receiver, Sender},
	thread,
	time::{Duration, Instant},
};
use windows::{
	Win32::{
		Foundation::FILETIME,
		System::{
			Performance::{
				PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE_ITEM_W,
				PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA, PdhAddEnglishCounterW,
				PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
			},
			SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX},
			Threading::GetSystemTimes,
		},
	},
	core::{PCWSTR, w},
};

#[derive(Clone, Copy, Default)]
struct CpuTimes {
	idle: u64,
	kernel: u64,
	user: u64,
}

impl CpuTimes {
	fn read() -> Option<Self> {
		let mut idle = FILETIME::default();
		let mut kernel = FILETIME::default();
		let mut user = FILETIME::default();
		// SAFETY: All three FILETIME output pointers are valid for this call.
		unsafe {
			GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).ok()?;
		}
		Some(Self {
			idle: filetime(idle),
			kernel: filetime(kernel),
			user: filetime(user),
		})
	}
}

fn filetime(value: FILETIME) -> u64 {
	(u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime)
}

const TEMPERATURE_REFRESH_INTERVAL: Duration = Duration::from_secs(5);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const TEMPERATURE_SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
$cpu = $null
$gpu = $null

foreach ($namespace in @('root\LibreHardwareMonitor', 'root\OpenHardwareMonitor')) {
	$sensors = @(Get-CimInstance -Namespace $namespace -ClassName Sensor -Filter "SensorType = 'Temperature'" -ErrorAction SilentlyContinue)
	if ($sensors.Count -eq 0) { continue }

	$cpuSensors = @($sensors | Where-Object { "$($_.Identifier) $($_.Parent)" -match '(?i)/(intel|amd)?cpu|cpu/' })
	$preferredCpu = @($cpuSensors | Where-Object { $_.Name -match '(?i)package|tctl|tdie|core max|cpu' })
	if ($preferredCpu.Count -eq 0) { $preferredCpu = $cpuSensors }
	if ($preferredCpu.Count -gt 0) {
		$cpu = [Math]::Round(($preferredCpu | Measure-Object -Property Value -Maximum).Maximum)
	}

	$gpuSensors = @($sensors | Where-Object { "$($_.Identifier) $($_.Parent)" -match '(?i)/(nvidia|amd|intel)?gpu|gpu/' })
	$preferredGpu = @($gpuSensors | Where-Object { $_.Name -match '(?i)gpu core|core|gpu temperature' })
	if ($preferredGpu.Count -eq 0) { $preferredGpu = $gpuSensors }
	if ($preferredGpu.Count -gt 0) {
		$gpu = [Math]::Round(($preferredGpu | Measure-Object -Property Value -Maximum).Maximum)
	}
	if ($null -ne $cpu -and $null -ne $gpu) { break }
}

if ($null -eq $cpu) {
	$zones = @(Get-CimInstance -Namespace root/wmi -ClassName MSAcpi_ThermalZoneTemperature -ErrorAction SilentlyContinue)
	$values = @($zones | ForEach-Object { ($_.CurrentTemperature / 10.0) - 273.15 } | Where-Object { $_ -ge 0 -and $_ -le 150 })
	if ($values.Count -gt 0) { $cpu = [Math]::Round(($values | Measure-Object -Maximum).Maximum) }
}

if ($null -eq $gpu) {
	$nvidiaSmi = Get-Command nvidia-smi -ErrorAction SilentlyContinue
	if ($null -ne $nvidiaSmi) {
		$values = @(& $nvidiaSmi.Source --query-gpu=temperature.gpu --format=csv,noheader,nounits 2>$null | ForEach-Object { [double]$_ })
		if ($LASTEXITCODE -eq 0 -and $values.Count -gt 0) { $gpu = [Math]::Round(($values | Measure-Object -Maximum).Maximum) }
	}
}

[Console]::Write("$cpu|$gpu")
"#;

struct TemperatureQuery {
	last_launch: Option<Instant>,
	cpu: Option<u8>,
	gpu: Option<u8>,
	in_flight: bool,
	sender: Sender<(Option<u8>, Option<u8>)>,
	receiver: Receiver<(Option<u8>, Option<u8>)>,
}

impl TemperatureQuery {
	fn new() -> Self {
		let (sender, receiver) = mpsc::channel();
		Self {
			last_launch: None,
			cpu: None,
			gpu: None,
			in_flight: false,
			sender,
			receiver,
		}
	}

	fn sample(&mut self) -> (Option<u8>, Option<u8>) {
		while let Ok((cpu, gpu)) = self.receiver.try_recv() {
			self.cpu = cpu;
			self.gpu = gpu;
			self.in_flight = false;
		}
		let refresh_due = self
			.last_launch
			.is_none_or(|last| last.elapsed() >= TEMPERATURE_REFRESH_INTERVAL);
		if refresh_due && !self.in_flight {
			let sender = self.sender.clone();
			self.last_launch = Some(Instant::now());
			self.in_flight = thread::Builder::new()
				.name("system-monitor-temperature".to_owned())
				.spawn(move || {
					let _ = sender.send(query_temperatures());
				})
				.is_ok();
		}
		(self.cpu, self.gpu)
	}
}

fn query_temperatures() -> (Option<u8>, Option<u8>) {
	let output = Command::new("powershell.exe")
		.args([
			"-NoLogo",
			"-NoProfile",
			"-NonInteractive",
			"-ExecutionPolicy",
			"Bypass",
			"-Command",
			TEMPERATURE_SCRIPT,
		])
		.stdin(Stdio::null())
		.stderr(Stdio::null())
		.creation_flags(CREATE_NO_WINDOW)
		.output();
	let Some(output) = output.ok().filter(|output| output.status.success()) else {
		return (None, None);
	};
	let output = String::from_utf8_lossy(&output.stdout);
	let mut values = output.trim().split('|');
	(
		parse_temperature(values.next()),
		parse_temperature(values.next()),
	)
}

fn parse_temperature(value: Option<&str>) -> Option<u8> {
	value?
		.trim()
		.parse::<f32>()
		.ok()
		.filter(|value| value.is_finite() && (0.0..=150.0).contains(value))
		.map(|value| value.round() as u8)
}

struct GpuQuery {
	query: PDH_HQUERY,
	counter: PDH_HCOUNTER,
}

impl GpuQuery {
	fn new() -> Option<Self> {
		let mut query = PDH_HQUERY::default();
		// SAFETY: The output handle is valid and a null data source selects realtime data.
		if unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut query) } != 0 {
			return None;
		}
		let mut counter = PDH_HCOUNTER::default();
		// SAFETY: The query is open and the wildcard English counter path is static.
		if unsafe {
			PdhAddEnglishCounterW(
				query,
				w!(r"\GPU Engine(*)\Utilization Percentage"),
				0,
				&mut counter,
			)
		} != 0
		{
			// SAFETY: query was successfully opened above.
			unsafe {
				PdhCloseQuery(query);
			}
			return None;
		}
		// Prime rate counters; the next collection produces a formatted value.
		// SAFETY: query and counter remain valid for the lifetime of this object.
		unsafe {
			PdhCollectQueryData(query);
		}
		Some(Self { query, counter })
	}

	fn sample(&mut self) -> Option<u8> {
		// SAFETY: The query remains valid until Drop.
		if unsafe { PdhCollectQueryData(self.query) } != 0 {
			return None;
		}
		let mut bytes = 0;
		let mut item_count = 0;
		// SAFETY: A null item buffer is the documented size-query call.
		let status = unsafe {
			PdhGetFormattedCounterArrayW(
				self.counter,
				PDH_FMT_DOUBLE,
				&mut bytes,
				&mut item_count,
				None,
			)
		};
		if status != PDH_MORE_DATA || bytes == 0 || item_count == 0 {
			return None;
		}

		let item_size = size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
		let slots = (bytes as usize).div_ceil(item_size);
		let mut buffer = vec![MaybeUninit::<PDH_FMT_COUNTERVALUE_ITEM_W>::uninit(); slots];
		// SAFETY: The aligned buffer has at least the byte size requested by PDH.
		if unsafe {
			PdhGetFormattedCounterArrayW(
				self.counter,
				PDH_FMT_DOUBLE,
				&mut bytes,
				&mut item_count,
				Some(buffer.as_mut_ptr().cast()),
			)
		} != 0
		{
			return None;
		}

		// SAFETY: PDH initialized item_count entries in the aligned buffer.
		let items = unsafe {
			std::slice::from_raw_parts(
				buffer.as_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>(),
				item_count as usize,
			)
		};
		let mut engines: HashMap<String, f64> = HashMap::new();
		for item in items {
			if !matches!(
				item.FmtValue.CStatus,
				PDH_CSTATUS_VALID_DATA | PDH_CSTATUS_NEW_DATA
			) {
				continue;
			}
			// SAFETY: PDH returns a null-terminated instance name in this buffer.
			let name = unsafe { item.szName.to_string() }.unwrap_or_default();
			let engine = name
				.find("_luid_")
				.map(|position| &name[position..])
				.unwrap_or(&name);
			// SAFETY: PDH_FMT_DOUBLE selects the doubleValue union member.
			let value = unsafe { item.FmtValue.Anonymous.doubleValue };
			*engines.entry(engine.to_owned()).or_default() += value.max(0.0);
		}
		engines
			.into_values()
			.reduce(f64::max)
			.map(|value| value.clamp(0.0, 100.0).round() as u8)
	}
}

impl Drop for GpuQuery {
	fn drop(&mut self) {
		// SAFETY: This handle was opened once and is closed exactly once here.
		unsafe {
			PdhCloseQuery(self.query);
		}
	}
}

pub struct Sampler {
	previous_cpu: CpuTimes,
	gpu: Option<GpuQuery>,
	temperature: TemperatureQuery,
}

impl Sampler {
	pub fn new() -> Self {
		Self {
			previous_cpu: CpuTimes::read().unwrap_or_default(),
			gpu: GpuQuery::new(),
			temperature: TemperatureQuery::new(),
		}
	}

	pub fn snapshot(&mut self) -> MonitorSnapshot {
		let current = CpuTimes::read().unwrap_or(self.previous_cpu);
		let idle = current.idle.saturating_sub(self.previous_cpu.idle);
		let kernel = current.kernel.saturating_sub(self.previous_cpu.kernel);
		let user = current.user.saturating_sub(self.previous_cpu.user);
		let total = kernel.saturating_add(user);
		let cpu = total
			.saturating_sub(idle)
			.saturating_mul(100)
			.checked_div(total)
			.unwrap_or_default()
			.min(100) as u8;
		self.previous_cpu = current;

		let mut memory_status = MEMORYSTATUSEX {
			dwLength: size_of::<MEMORYSTATUSEX>() as u32,
			..Default::default()
		};
		// SAFETY: memory_status has the required size field and is valid for output.
		let memory_available = unsafe { GlobalMemoryStatusEx(&mut memory_status) }.is_ok();
		let (
			memory,
			memory_total_mib,
			memory_available_mib,
			pagefile_total_mib,
			pagefile_available_mib,
		) = if memory_available {
			(
				memory_status.dwMemoryLoad.min(100) as u8,
				memory_status.ullTotalPhys / (1024 * 1024),
				memory_status.ullAvailPhys / (1024 * 1024),
				memory_status.ullTotalPageFile / (1024 * 1024),
				memory_status.ullAvailPageFile / (1024 * 1024),
			)
		} else {
			(0, 0, 0, 0, 0)
		};
		let gpu = self.gpu.as_mut().and_then(GpuQuery::sample);
		let (cpu_temperature, gpu_temperature) = self.temperature.sample();

		MonitorSnapshot {
			cpu,
			gpu,
			cpu_temperature,
			gpu_temperature,
			memory,
			memory_total_mib,
			memory_available_mib,
			pagefile_total_mib,
			pagefile_available_mib,
		}
	}
}
