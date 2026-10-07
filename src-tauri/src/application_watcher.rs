use crate::store::{NotProfile, Store};

use std::collections::HashMap;
#[cfg(target_os = "windows")]
use std::ffi::OsStr;

use active_win_pos_rs::get_active_window;
use once_cell::sync::Lazy;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};
use tauri::{Emitter, Manager};
use tokio::sync::RwLock;

pub type ApplicationProfiles = HashMap<String, HashMap<String, String>>;
impl NotProfile for ApplicationProfiles {}

pub static APPLICATIONS: RwLock<Vec<String>> = RwLock::const_new(Vec::new());
pub static APPLICATION_PROFILES: Lazy<RwLock<Store<ApplicationProfiles>>> = Lazy::new(|| RwLock::new(Store::new("applications", &crate::shared::config_dir(), HashMap::new()).unwrap()));

pub static APPLICATION_PROCESSES: Lazy<RwLock<HashMap<String, Vec<u32>>>> = Lazy::new(|| RwLock::new(HashMap::new()));
pub static APPLICATION_PLUGINS: Lazy<RwLock<HashMap<String, Vec<String>>>> = Lazy::new(|| RwLock::new(HashMap::new()));
pub static PREVIOUS_PROFILES: Lazy<RwLock<HashMap<String, String>>> = Lazy::new(|| RwLock::new(HashMap::new()));

#[cfg(target_os = "windows")]
fn is_microsoft_teams_process(name: &OsStr) -> bool {
	matches!(name.to_string_lossy().to_ascii_lowercase().as_str(), "ms-teams.exe" | "msteams.exe" | "teams.exe")
}

#[derive(Clone, serde::Serialize)]
pub struct SwitchProfileEvent {
	device: String,
	profile: String,
}

async fn track_active_application(app_handle: &tauri::AppHandle) -> String {
	let Ok(win) = get_active_window() else {
		return String::new();
	};
	let mut applications = APPLICATIONS.write().await;
	if !applications.contains(&win.app_name) && !win.app_name.to_lowercase().starts_with("opendeck") && !win.app_name.trim().is_empty() {
		applications.push(win.app_name.clone());
		let _ = app_handle.get_webview_window("main").unwrap().emit("applications", applications.clone());
	}
	win.app_name
}

/// Decides which profile a device should switch to for the active application.
/// Returns `(profile, remember_current, restore_previous)`, or `None` if nothing applies.
fn resolve_application_profile(
	application: Option<&HashMap<String, String>>,
	default: Option<&HashMap<String, String>>,
	device: &str,
	previous_profile: Option<String>,
) -> Option<(String, bool, bool)> {
	if let Some(profile) = application.and_then(|profiles| profiles.get(device)).cloned() {
		return Some((profile, previous_profile.is_none(), false));
	}
	if let Some(profile) = previous_profile {
		return Some((profile, false, true));
	}
	default.and_then(|profiles| profiles.get(device)).cloned().map(|profile| (profile, false, false))
}

async fn switch_device_profile(app_handle: &tauri::AppHandle, device: &str, profile: &str, app_name: &str) -> bool {
	match crate::events::frontend::profiles::set_selected_profile(device.to_owned(), profile.to_owned()).await {
		Ok(()) => {
			let _ = app_handle.get_webview_window("main").unwrap().emit(
				"switch_profile",
				SwitchProfileEvent {
					device: device.to_owned(),
					profile: profile.to_owned(),
				},
			);
			true
		}
		Err(error) => {
			log::error!("Failed to switch device {device} to profile {profile} for application {app_name}: {error}");
			false
		}
	}
}

async fn apply_application_profiles(app_handle: &tauri::AppHandle, app_name: &str) {
	let application_profiles = APPLICATION_PROFILES.read().await.value.clone();
	let application = application_profiles.get(app_name);
	let default = application_profiles.get("opendeck_default");
	let devices: Vec<String> = crate::shared::DEVICES.iter().map(|value| value.key().clone()).collect();
	for device in devices {
		let Ok(current_profile) = crate::store::profiles::DEVICE_STORES.write().await.get_selected_profile(&device) else {
			continue;
		};

		let previous_profile = PREVIOUS_PROFILES.read().await.get(&device).cloned();
		let Some((profile, remember_current, restore_previous)) = resolve_application_profile(application, default, &device, previous_profile) else {
			continue;
		};

		if current_profile != profile && !switch_device_profile(app_handle, &device, &profile, app_name).await {
			continue;
		}

		if remember_current {
			PREVIOUS_PROFILES.write().await.insert(device.clone(), current_profile);
		} else if restore_previous {
			PREVIOUS_PROFILES.write().await.remove(&device);
		}
	}
}

async fn watch_active_application() {
	let mut previous = String::new();
	let app_handle = crate::APP_HANDLE.get().unwrap();
	loop {
		let app_name = track_active_application(app_handle).await;
		if app_name != previous {
			apply_application_profiles(app_handle, &app_name).await;
			previous = app_name;
		}

		tokio::time::sleep(std::time::Duration::from_millis(250)).await;
	}
}

/// Drops processes that have exited and notifies the plugins monitoring them.
async fn prune_terminated_processes(system: &System) {
	for (application, processes) in APPLICATION_PROCESSES.write().await.iter_mut() {
		let mut alive_processes = Vec::with_capacity(processes.len());
		for pid in processes.iter() {
			if system.process(Pid::from_u32(*pid)).is_some() {
				alive_processes.push(*pid);
				continue;
			}
			for plugin in APPLICATION_PLUGINS.read().await.get(application).into_iter().flatten() {
				let _ = crate::events::outbound::applications::application_did_terminate(plugin, application.clone()).await;
			}
		}
		*processes = alive_processes;
	}
}

/// Records newly started processes and notifies the plugins monitoring them.
async fn detect_launched_processes(system: &System) {
	let application_plugins = APPLICATION_PLUGINS.read().await;
	for (application, plugins) in application_plugins.iter() {
		for process in system.processes_by_exact_name(application.as_ref()) {
			let pid = process.pid().as_u32();
			let mut application_processes = APPLICATION_PROCESSES.write().await;
			let pids = application_processes.entry(application.clone()).or_default();
			if pids.contains(&pid) {
				continue;
			}
			pids.push(pid);
			for plugin in plugins {
				let _ = crate::events::outbound::applications::application_did_launch(plugin, application.clone()).await;
			}
		}
	}
}

async fn watch_application_processes() {
	let mut system = System::new_with_specifics(RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing().without_tasks()));
	#[cfg(target_os = "windows")]
	let mut microsoft_teams_was_running = None;

	loop {
		system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().without_tasks());
		#[cfg(target_os = "windows")]
		let microsoft_teams_running = system.processes().values().any(|process| is_microsoft_teams_process(process.name()));

		prune_terminated_processes(&system).await;
		detect_launched_processes(&system).await;

		#[cfg(target_os = "windows")]
		{
			// Keep checking the stopped state so a concurrently initialising or
			// manually reloaded Teams plugin is shut down on the next pass.
			if (!microsoft_teams_running || microsoft_teams_was_running == Some(false))
				&& let Err(error) = crate::plugins::set_microsoft_teams_running(microsoft_teams_running).await
			{
				log::warn!("Failed to update Microsoft Teams plugin lifecycle: {error}");
			}
			microsoft_teams_was_running = Some(microsoft_teams_running);
		}

		tokio::time::sleep(std::time::Duration::from_millis(250)).await;
	}
}

pub fn init_application_watcher() {
	tokio::spawn(watch_active_application());
	tokio::spawn(watch_application_processes());
}

pub async fn start_monitoring(plugin: &str, applications: &Vec<String>) {
	let mut application_plugins = APPLICATION_PLUGINS.write().await;

	for application in applications {
		application_plugins.entry(application.to_owned()).or_default().push(plugin.to_owned());

		let application_processes = APPLICATION_PROCESSES.read().await;
		if let Some(pids) = application_processes.get(application) {
			for _ in pids {
				let _ = crate::events::outbound::applications::application_did_launch(plugin, application.to_owned()).await;
			}
		}
	}
}

pub async fn stop_monitoring(plugin: &str) {
	let mut application_plugins = APPLICATION_PLUGINS.write().await;
	for plugins in application_plugins.values_mut() {
		plugins.retain(|p| p != plugin);
	}
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
	use super::is_microsoft_teams_process;
	use std::ffi::OsStr;

	#[test]
	fn detects_new_classic_and_store_teams_processes_case_insensitively() {
		for name in ["ms-teams.exe", "MS-TEAMS.EXE", "Teams.exe", "msteams.exe"] {
			assert!(is_microsoft_teams_process(OsStr::new(name)));
		}
		assert!(!is_microsoft_teams_process(OsStr::new("TeamsUpdater.exe")));
	}
}
