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
