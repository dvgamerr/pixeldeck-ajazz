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

fn sample_loop(sender: mpsc::Sender<MonitorSnapshot>, cancel: &AtomicBool) {
	let mut sampler = platform::Sampler::new();
	std::thread::sleep(Duration::from_millis(250));
	while !cancel.load(Ordering::Acquire) {
		if sender.blocking_send(sampler.snapshot()).is_err() {
			break;
		}
		for _ in 0..10 {
			if cancel.load(Ordering::Acquire) {
				return;
			}
			std::thread::sleep(SAMPLE_INTERVAL / 10);
		}
	}
}

fn spawn_sampler(
	sender: mpsc::Sender<MonitorSnapshot>,
	cancel: Arc<AtomicBool>,
) -> std::io::Result<()> {
	std::thread::Builder::new()
		.name("system-monitor-sampler".to_owned())
		.spawn(move || sample_loop(sender, &cancel))
		.map(drop)
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
	let (sender, receiver) = mpsc::channel(2);
	if let Err(error) = spawn_sampler(sender, cancel.clone()) {
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
#[path = "system_monitor/platform_windows.rs"]
mod platform;

#[cfg(not(windows))]
#[path = "system_monitor/platform_other.rs"]
mod platform;

#[cfg(test)]
mod tests;
