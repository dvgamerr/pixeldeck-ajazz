use crate::compat::{EventHandlerResult, OutboundEventManager, SettingsValue};
use plotters::prelude::{ChartBuilder, IntoDrawingArea, LineSeries, RGBColor, SVGBackend};
use std::{
	collections::{HashMap, VecDeque},
	sync::{
		Arc, LazyLock, Mutex,
		atomic::{AtomicBool, Ordering},
	},
	time::Duration,
};
use tokio::sync::mpsc;

use crate::{
	live::{self, LiveAction},
	pixel::{data_uri, text_path},
};

pub const ACTION: &str = "com.amansprojects.starterpack.systemmonitor";
// Two seconds keeps the dashboard responsive while halving continuous image
// encoding and USB traffic compared with the previous one-second cadence.
const SAMPLE_INTERVAL: Duration = Duration::from_secs(2);
const HISTORY_LENGTH: usize = 30;

#[derive(Clone, Debug, PartialEq)]
struct MonitorSnapshot {
	cpu: u8,
	gpu: Option<u8>,
	cpu_temperature: Option<u8>,
	gpu_temperature: Option<u8>,
	memory: u8,
	memory_total_mib: u64,
	memory_available_mib: u64,
	pagefile_total_mib: u64,
	pagefile_available_mib: u64,
}

impl MonitorSnapshot {
	fn memory_used_mib(&self) -> u64 {
		self.memory_total_mib
			.saturating_sub(self.memory_available_mib)
	}

	fn pagefile_used_mib(&self) -> u64 {
		self.pagefile_total_mib
			.saturating_sub(self.pagefile_available_mib)
	}
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DisplayMode {
	Compact,
	#[default]
	Normal,
	Full,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DisplayMetric {
	#[default]
	Overview,
	Cpu,
	Gpu,
	Memory,
	CpuTemperature,
	GpuTemperature,
}

impl DisplayMetric {
	fn from_value(value: Option<&SettingsValue>) -> Self {
		match value.and_then(SettingsValue::as_str) {
			Some("cpu") => Self::Cpu,
			Some("gpu") => Self::Gpu,
			Some("memory" | "ram") => Self::Memory,
			Some("cpu-temperature" | "cpu_temp") => Self::CpuTemperature,
			Some("gpu-temperature" | "gpu_temp") => Self::GpuTemperature,
			_ => Self::Overview,
		}
	}

	fn label(self) -> &'static str {
		match self {
			Self::Overview => "SYSTEM",
			Self::Cpu => "CPU USAGE",
			Self::Gpu => "GPU USAGE",
			Self::Memory => "MEMORY",
			Self::CpuTemperature => "CPU TEMP",
			Self::GpuTemperature => "GPU TEMP",
		}
	}

	fn short_label(self) -> &'static str {
		match self {
			Self::Overview => "SYS",
			Self::Cpu => "CPU",
			Self::Gpu => "GPU",
			Self::Memory => "RAM",
			Self::CpuTemperature => "CPU TEMP",
			Self::GpuTemperature => "GPU TEMP",
		}
	}

	fn color(self) -> &'static str {
		match self {
			Self::Overview | Self::Cpu => "#20e3ff",
			Self::Gpu => "#ff4fd8",
			Self::Memory => "#facc15",
			Self::CpuTemperature => "#fb923c",
			Self::GpuTemperature => "#a3e635",
		}
	}

	fn value(self, snapshot: &MonitorSnapshot) -> Option<u8> {
		match self {
			Self::Overview => None,
			Self::Cpu => Some(snapshot.cpu),
			Self::Gpu => snapshot.gpu,
			Self::Memory => Some(snapshot.memory),
			Self::CpuTemperature => snapshot.cpu_temperature,
			Self::GpuTemperature => snapshot.gpu_temperature,
		}
	}

	fn formatted_value(self, snapshot: &MonitorSnapshot) -> String {
		self.value(snapshot)
			.map(|value| {
				if matches!(self, Self::CpuTemperature | Self::GpuTemperature) {
					format!("{value} C")
				} else {
					format!("{value}%")
				}
			})
			.unwrap_or_else(|| "--".to_owned())
	}
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct MonitorSettings {
	mode: DisplayMode,
	metric: DisplayMetric,
}

impl MonitorSettings {
	fn from_settings(settings: &SettingsValue) -> Self {
		let settings = settings.as_object();
		let mode = match settings
			.and_then(|settings| settings.get("mode"))
			.and_then(SettingsValue::as_str)
		{
			Some("compact" | "mini") => DisplayMode::Compact,
			Some("full") => DisplayMode::Full,
			_ => DisplayMode::Normal,
		};
		let metric =
			DisplayMetric::from_value(settings.and_then(|settings| settings.get("metric")));
		Self { mode, metric }
	}
}

#[derive(Clone, Copy, Debug)]
struct HistoryPoint {
	cpu: u8,
	gpu: Option<u8>,
	cpu_temperature: Option<u8>,
	gpu_temperature: Option<u8>,
	memory: u8,
}

impl From<&MonitorSnapshot> for HistoryPoint {
	fn from(snapshot: &MonitorSnapshot) -> Self {
		Self {
			cpu: snapshot.cpu,
			gpu: snapshot.gpu,
			cpu_temperature: snapshot.cpu_temperature,
			gpu_temperature: snapshot.gpu_temperature,
			memory: snapshot.memory,
		}
	}
}

#[derive(Clone)]
struct RenderState {
	snapshot: MonitorSnapshot,
	history: VecDeque<HistoryPoint>,
}

static LIVE: LazyLock<Mutex<LiveAction>> = LazyLock::new(Default::default);
static SETTINGS: LazyLock<Mutex<HashMap<String, MonitorSettings>>> =
	LazyLock::new(Default::default);
static LAST_STATE: LazyLock<Mutex<Option<RenderState>>> = LazyLock::new(Default::default);

fn meter(label: &str, value: Option<u8>, color: &str, y: u8) -> String {
	const SEGMENTS: usize = 12;
	let active = value
		.map(|value| (usize::from(value) * SEGMENTS).div_ceil(100))
		.unwrap_or_default();
	let value = value
		.map(|value| format!("{value}%"))
		.unwrap_or_else(|| "--".to_owned());
	let label = text_path(label, 20.0, y + 13, 10, color);
	let value = text_path(&value, 47.0, y + 27, 16, "#f8fafc");
	let mut bars = String::with_capacity(SEGMENTS * 70);
	for index in 0..SEGMENTS {
		let x = 66 + index * 9;
		let fill = if index < active { color } else { "#171c24" };
		bars.push_str(&format!(
			r##"<rect x="{x}" y="{}" width="7" height="16" fill="{fill}"/>"##,
			y + 8
		));
	}
	format!("{label}{value}{bars}")
}

fn medium_image(snapshot: &MonitorSnapshot) -> String {
	let cpu = meter("CPU", Some(snapshot.cpu), "#20e3ff", 3);
	let gpu = meter("GPU", snapshot.gpu, "#ff4fd8", 39);
	let memory = meter("RAM", Some(snapshot.memory), "#facc15", 75);
	let svg = format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" width="176" height="112" viewBox="0 0 176 112" shape-rendering="crispEdges">
<rect width="176" height="112" fill="#000"/>
{cpu}{gpu}{memory}
</svg>"##
	);
	data_uri(&svg)
}

fn compact_image(snapshot: &MonitorSnapshot) -> String {
	let labels = [
		text_path("CPU", 29.0, 44, 13, "#20e3ff"),
		text_path("GPU", 88.0, 44, 13, "#ff4fd8"),
		text_path("MEM", 147.0, 44, 13, "#facc15"),
	];
	let values = [
		text_path(&format!("{}%", snapshot.cpu), 29.0, 73, 18, "#f8fafc"),
		text_path(
			&snapshot
				.gpu
				.map(|value| format!("{value}%"))
				.unwrap_or_else(|| "--".to_owned()),
			88.0,
			72,
			18,
			"#f8fafc",
		),
		text_path(
			&format!("{}GB", (snapshot.memory_used_mib() + 512) / 1024),
			147.0,
			72,
			18,
			"#f8fafc",
		),
	];
	data_uri(&format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" width="176" height="112" viewBox="0 0 176 112" shape-rendering="crispEdges">
<rect width="176" height="112" fill="#000"/>
<path d="M58 27v58M117 27v58" stroke="#171c24"/>
{}{}
</svg>"##,
		labels.concat(),
		values.concat()
	))
}

fn gibibytes(mib: u64) -> String {
	format!("{:.1}", mib as f64 / 1024.0)
}

fn chart_fragment(history: &VecDeque<HistoryPoint>) -> Option<String> {
	let mut svg = String::new();
	{
		let root = SVGBackend::with_string(&mut svg, (172, 43)).into_drawing_area();
		root.fill(&RGBColor(0, 0, 0)).ok()?;
		let mut chart = ChartBuilder::on(&root)
			.margin(1)
			.build_cartesian_2d(0i32..(HISTORY_LENGTH as i32 - 1), 0i32..100i32)
			.ok()?;
		let offset = HISTORY_LENGTH.saturating_sub(history.len());
		let points = |value: fn(&HistoryPoint) -> Option<u8>| {
			history
				.iter()
				.enumerate()
				.filter_map(move |(index, point)| {
					value(point).map(|value| ((offset + index) as i32, i32::from(value)))
				})
		};
		chart
			.draw_series(LineSeries::new(
				points(|point| Some(point.cpu)),
				RGBColor(32, 227, 255),
			))
			.ok()?;
		chart
			.draw_series(LineSeries::new(
				points(|point| point.gpu),
				RGBColor(255, 79, 216),
			))
			.ok()?;
		chart
			.draw_series(LineSeries::new(
				points(|point| Some(point.memory)),
				RGBColor(250, 204, 21),
			))
			.ok()?;
		root.present().ok()?;
	}
	let start = svg.find('>')? + 1;
	let end = svg.rfind("</svg>")?;
	Some(svg[start..end].to_owned())
}

fn metric_chart_fragment(
	history: &VecDeque<HistoryPoint>,
	metric: DisplayMetric,
) -> Option<String> {
	let mut svg = String::new();
	{
		let root = SVGBackend::with_string(&mut svg, (168, 64)).into_drawing_area();
		root.fill(&RGBColor(0, 0, 0)).ok()?;
		let mut chart = ChartBuilder::on(&root)
			.margin(1)
			.build_cartesian_2d(0i32..(HISTORY_LENGTH as i32 - 1), 0i32..100i32)
			.ok()?;
		let offset = HISTORY_LENGTH.saturating_sub(history.len());
		let points = history.iter().enumerate().filter_map(|(index, point)| {
			let value = match metric {
				DisplayMetric::Cpu => Some(point.cpu),
				DisplayMetric::Gpu => point.gpu,
				DisplayMetric::Memory => Some(point.memory),
				DisplayMetric::CpuTemperature => point.cpu_temperature,
				DisplayMetric::GpuTemperature => point.gpu_temperature,
				DisplayMetric::Overview => None,
			};
			value.map(|value| ((offset + index) as i32, i32::from(value.min(100))))
		});
		let color = match metric {
			DisplayMetric::Gpu => RGBColor(255, 79, 216),
			DisplayMetric::Memory => RGBColor(250, 204, 21),
			DisplayMetric::CpuTemperature => RGBColor(251, 146, 60),
			DisplayMetric::GpuTemperature => RGBColor(163, 230, 53),
			_ => RGBColor(32, 227, 255),
		};
		chart.draw_series(LineSeries::new(points, color)).ok()?;
		root.present().ok()?;
	}
	let start = svg.find('>')? + 1;
	let end = svg.rfind("</svg>")?;
	Some(svg[start..end].to_owned())
}

fn full_image(snapshot: &MonitorSnapshot, history: &VecDeque<HistoryPoint>) -> String {
	let headings = [
		text_path("CPU", 29.0, 10, 8, "#20e3ff"),
		text_path("GPU", 88.0, 10, 8, "#ff4fd8"),
		text_path("RAM", 147.0, 10, 8, "#facc15"),
	];
	let values = [
		text_path(&format!("{}%", snapshot.cpu), 29.0, 23, 11, "#f8fafc"),
		text_path(
			&snapshot
				.gpu
				.map(|value| format!("{value}%"))
				.unwrap_or_else(|| "--".to_owned()),
			88.0,
			22,
			11,
			"#f8fafc",
		),
		text_path(&format!("{}%", snapshot.memory), 147.0, 23, 11, "#f8fafc"),
	];
	let ram = text_path(
		&format!(
			"RAM {} / {} GB",
			gibibytes(snapshot.memory_used_mib()),
			gibibytes(snapshot.memory_total_mib)
		),
		88.0,
		83,
		7,
		"#dbeafe",
	);
	let free = text_path(
		&format!(
			"FREE {} GB  LOAD {}%",
			gibibytes(snapshot.memory_available_mib),
			snapshot.memory
		),
		88.0,
		94,
		7,
		"#cbd5e1",
	);
	let page = text_path(
		&format!(
			"PAGE {} / {} GB",
			gibibytes(snapshot.pagefile_used_mib()),
			gibibytes(snapshot.pagefile_total_mib)
		),
		88.0,
		105,
		7,
		"#94a3b8",
	);
	let chart = chart_fragment(history).unwrap_or_default();
	data_uri(&format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" width="176" height="112" viewBox="0 0 176 112">
<rect width="176" height="112" fill="#000"/>
{}{}
<svg x="2" y="27" width="172" height="43" viewBox="0 0 172 43">{chart}</svg>
{ram}{free}{page}
</svg>"##,
		headings.concat(),
		values.concat(),
	))
}

fn single_compact_image(snapshot: &MonitorSnapshot, metric: DisplayMetric) -> String {
	let color = metric.color();
	let label = text_path(metric.label(), 88.0, 34, 16, color);
	let value = text_path(&metric.formatted_value(snapshot), 88.0, 82, 40, "#f8fafc");
	data_uri(&format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" width="176" height="112" viewBox="0 0 176 112" shape-rendering="crispEdges">
<rect width="176" height="112" fill="#000"/>
{label}{value}
</svg>"##
	))
}

fn single_normal_image(snapshot: &MonitorSnapshot, metric: DisplayMetric) -> String {
	const SEGMENTS: usize = 14;
	let color = metric.color();
	let raw_value = metric.value(snapshot);
	let active = raw_value
		.map(|value| (usize::from(value.min(100)) * SEGMENTS).div_ceil(100))
		.unwrap_or_default();
	let label = text_path(metric.label(), 88.0, 29, 16, color);
	let value = text_path(&metric.formatted_value(snapshot), 88.0, 73, 34, "#f8fafc");
	let mut bars = String::with_capacity(SEGMENTS * 70);
	for index in 0..SEGMENTS {
		let x = 8 + index * 12;
		let fill = if index < active { color } else { "#171c24" };
		bars.push_str(&format!(
			r##"<rect x="{x}" y="88" width="9" height="16" fill="{fill}"/>"##
		));
	}
	data_uri(&format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" width="176" height="112" viewBox="0 0 176 112" shape-rendering="crispEdges">
<rect width="176" height="112" fill="#000"/>
{label}{value}{bars}
</svg>"##
	))
}

fn single_full_image(
	snapshot: &MonitorSnapshot,
	metric: DisplayMetric,
	history: &VecDeque<HistoryPoint>,
) -> String {
	let color = metric.color();
	let label = text_path(metric.short_label(), 38.0, 22, 11, color);
	let value = text_path(&metric.formatted_value(snapshot), 126.0, 25, 22, "#f8fafc");
	let chart = metric_chart_fragment(history, metric).unwrap_or_default();
	data_uri(&format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" width="176" height="112" viewBox="0 0 176 112">
<rect width="176" height="112" fill="#000"/>
{label}{value}
<svg x="4" y="42" width="168" height="64" viewBox="0 0 168 64">{chart}</svg>
</svg>"##
	))
}

fn monitor_image(
	snapshot: &MonitorSnapshot,
	settings: MonitorSettings,
	history: &VecDeque<HistoryPoint>,
) -> String {
	if settings.metric == DisplayMetric::Overview {
		return match settings.mode {
			DisplayMode::Compact => compact_image(snapshot),
			DisplayMode::Normal => medium_image(snapshot),
			DisplayMode::Full => full_image(snapshot, history),
		};
	}
	match settings.mode {
		DisplayMode::Compact => single_compact_image(snapshot, settings.metric),
		DisplayMode::Normal => single_normal_image(snapshot, settings.metric),
		DisplayMode::Full => single_full_image(snapshot, settings.metric, history),
	}
}

fn loading_image() -> String {
	let title = text_path("SYSTEM", 88.0, 49, 17, "#20e3ff");
	let status = text_path("LOADING...", 88.0, 72, 10, "#a7b0c0");
	data_uri(&format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" width="176" height="112" viewBox="0 0 176 112" shape-rendering="crispEdges">
<rect width="176" height="112" fill="#000"/>
{title}{status}
</svg>"##
	))
}

fn appearance_image(settings: MonitorSettings, state: Option<&RenderState>) -> String {
	state
		.map(|state| monitor_image(&state.snapshot, settings, &state.history))
		.unwrap_or_else(loading_image)
}

async fn render(mut receiver: mpsc::Receiver<MonitorSnapshot>) {
	let mut history = LAST_STATE
		.lock()
		.unwrap()
		.as_ref()
		.map(|state| state.history.clone())
		.unwrap_or_else(|| VecDeque::with_capacity(HISTORY_LENGTH));
	while let Some(snapshot) = receiver.recv().await {
		if history.len() == HISTORY_LENGTH {
			history.pop_front();
		}
		history.push_back(HistoryPoint::from(&snapshot));
		*LAST_STATE.lock().unwrap() = Some(RenderState {
			snapshot: snapshot.clone(),
			history: history.clone(),
		});
		let settings = SETTINGS.lock().unwrap().clone();
		if let Err(error) = live::broadcast_mapped(&LIVE, |context| {
			monitor_image(
				&snapshot,
				settings.get(context).copied().unwrap_or_default(),
				&history,
			)
		})
		.await
		{
			log::warn!("System monitor image update failed: {error}");
			break;
		}
	}
}

pub async fn appear(
	context: String,
	settings: SettingsValue,
	outbound: &mut OutboundEventManager,
) -> EventHandlerResult {
	let settings = MonitorSettings::from_settings(&settings);
	SETTINGS.lock().unwrap().insert(context.clone(), settings);
	let state = LAST_STATE.lock().unwrap().clone();
	outbound
		.set_image(
			context.clone(),
			Some(appearance_image(settings, state.as_ref())),
			None,
		)
		.await?;

	let mut live = LIVE.lock().unwrap();
	if !live.subscribe(context.clone()) {
		return Ok(());
	}
	let cancel = Arc::new(AtomicBool::new(false));
	let sampler_cancel = cancel.clone();
	let (sender, receiver) = mpsc::channel(2);
	if let Err(error) = std::thread::Builder::new()
		.name("system-monitor-sampler".to_owned())
		.spawn(move || {
			let mut sampler = platform::Sampler::new();
			std::thread::sleep(Duration::from_millis(250));
			while !sampler_cancel.load(Ordering::Acquire) {
				if sender.blocking_send(sampler.snapshot()).is_err() {
					break;
				}
				for _ in 0..10 {
					if sampler_cancel.load(Ordering::Acquire) {
						return;
					}
					std::thread::sleep(SAMPLE_INTERVAL / 10);
				}
			}
		}) {
		live.unsubscribe(&context);
		return Err(error.into());
	}
	live.start(tokio::spawn(render(receiver)), cancel);
	Ok(())
}

pub async fn refresh(
	context: String,
	settings: SettingsValue,
	outbound: &mut OutboundEventManager,
) -> EventHandlerResult {
	let settings = MonitorSettings::from_settings(&settings);
	SETTINGS.lock().unwrap().insert(context.clone(), settings);
	let state = LAST_STATE.lock().unwrap().clone();
	if let Some(state) = state {
		outbound
			.set_image(
				context,
				Some(monitor_image(&state.snapshot, settings, &state.history)),
				None,
			)
			.await?;
	}
	Ok(())
}

pub fn disappear(context: &str) {
	SETTINGS.lock().unwrap().remove(context);
	LIVE.lock().unwrap().unsubscribe(context);
}

#[cfg(windows)]
mod platform {
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
					PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
					PdhOpenQueryW,
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
}

#[cfg(not(windows))]
mod platform {
	use super::MonitorSnapshot;

	pub struct Sampler;

	impl Sampler {
		pub fn new() -> Self {
			Self
		}

		pub fn snapshot(&mut self) -> MonitorSnapshot {
			MonitorSnapshot {
				cpu: 0,
				gpu: None,
				cpu_temperature: None,
				gpu_temperature: None,
				memory: 0,
				memory_total_mib: 0,
				memory_available_mib: 0,
				pagefile_total_mib: 0,
				pagefile_available_mib: 0,
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::{
		DisplayMetric, DisplayMode, HistoryPoint, MonitorSettings, MonitorSnapshot, RenderState,
		appearance_image, loading_image, monitor_image,
	};
	use std::collections::VecDeque;

	fn snapshot() -> MonitorSnapshot {
		MonitorSnapshot {
			cpu: 23,
			gpu: Some(67),
			cpu_temperature: Some(58),
			gpu_temperature: Some(71),
			memory: 81,
			memory_total_mib: 32 * 1024,
			memory_available_mib: 6 * 1024,
			pagefile_total_mib: 40 * 1024,
			pagefile_available_mib: 18 * 1024,
		}
	}

	fn history(snapshot: &MonitorSnapshot) -> VecDeque<HistoryPoint> {
		VecDeque::from([HistoryPoint::from(snapshot)])
	}

	#[test]
	fn image_is_native_pixel_art_with_all_three_metrics() {
		let snapshot = snapshot();
		let image = monitor_image(&snapshot, MonitorSettings::default(), &history(&snapshot));

		assert!(image.starts_with("data:image/svg+xml,"));
		assert!(image.contains("width%3D%22176%22"));
		assert!(image.contains("height%3D%22112%22"));
		assert!(image.contains("shape-rendering%3D%22crispEdges%22"));
		assert!(image.contains("%2320e3ff"));
		assert!(image.contains("%23ff4fd8"));
		assert!(image.contains("%23facc15"));
		assert!(!image.contains("%3Ctext"));
	}

	#[test]
	fn unavailable_gpu_is_rendered_without_panicking() {
		let snapshot = MonitorSnapshot {
			cpu: 1,
			gpu: None,
			cpu_temperature: None,
			gpu_temperature: None,
			memory: 2,
			memory_total_mib: 16 * 1024,
			memory_available_mib: 12 * 1024,
			pagefile_total_mib: 20 * 1024,
			pagefile_available_mib: 10 * 1024,
		};
		let image = monitor_image(&snapshot, MonitorSettings::default(), &history(&snapshot));
		assert!(image.len() < 50_000);
	}

	#[test]
	fn compact_mode_stays_compact_and_full_mode_contains_plotters_lines() {
		let snapshot = snapshot();
		let history = history(&snapshot);
		let compact = monitor_image(
			&snapshot,
			MonitorSettings {
				mode: DisplayMode::Compact,
				..Default::default()
			},
			&history,
		);
		let full = monitor_image(
			&snapshot,
			MonitorSettings {
				mode: DisplayMode::Full,
				..Default::default()
			},
			&history,
		);

		assert!(compact.len() < 30_000);
		assert!(full.contains("%3Cpolyline") || full.contains("%3Cpath"));
		assert!(full.contains("%23ff4fd8"));
	}

	#[test]
	fn display_mode_settings_accept_current_and_legacy_names() {
		assert_eq!(
			MonitorSettings::from_settings(&serde_json::json!({ "mode": "compact" })).mode,
			DisplayMode::Compact
		);
		assert_eq!(
			MonitorSettings::from_settings(&serde_json::json!({ "mode": "normal" })).mode,
			DisplayMode::Normal
		);
		assert_eq!(
			MonitorSettings::from_settings(&serde_json::json!({ "mode": "full" })).mode,
			DisplayMode::Full
		);
		assert_eq!(
			MonitorSettings::from_settings(&serde_json::json!({ "mode": "mini" })).mode,
			DisplayMode::Compact
		);
		assert_eq!(
			MonitorSettings::from_settings(&serde_json::json!({ "mode": "medium" })).mode,
			DisplayMode::Normal
		);
	}

	#[test]
	fn metric_settings_render_large_temperature_views() {
		let snapshot = snapshot();
		let history = history(&snapshot);
		let settings = MonitorSettings::from_settings(&serde_json::json!({
			"mode": "normal",
			"metric": "cpu-temperature"
		}));
		assert_eq!(settings.metric, DisplayMetric::CpuTemperature);
		let image = monitor_image(&snapshot, settings, &history);

		assert!(image.contains("%23fb923c"));
		assert!(image.contains("width%3D%229%22%20height%3D%2216%22"));
		assert!(!image.contains("%23080b10"));
		assert!(!image.contains("%23202938"));
		assert!(!image.contains("%231e293b"));
		assert!(image.len() < 40_000);
	}

	#[test]
	fn appearance_reuses_the_last_rendered_state() {
		let snapshot = snapshot();
		let history = history(&snapshot);
		let state = RenderState { snapshot, history };
		let settings = MonitorSettings::default();
		assert_eq!(
			appearance_image(settings, Some(&state)),
			monitor_image(&state.snapshot, settings, &state.history)
		);
	}

	#[test]
	fn appearance_shows_loading_without_a_rendered_state() {
		assert_eq!(
			appearance_image(MonitorSettings::default(), None),
			loading_image()
		);
	}

	#[cfg(windows)]
	#[test]
	fn windows_system_snapshot_is_readable() {
		let mut sampler = super::platform::Sampler::new();
		std::thread::sleep(std::time::Duration::from_millis(250));
		let snapshot = sampler.snapshot();

		assert!(snapshot.cpu <= 100);
		assert!((1..=100).contains(&snapshot.memory));
		assert!(snapshot.gpu.is_none_or(|value| value <= 100));
		assert!(snapshot.cpu_temperature.is_none_or(|value| value <= 150));
		assert!(snapshot.gpu_temperature.is_none_or(|value| value <= 150));
	}
}
