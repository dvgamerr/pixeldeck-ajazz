pub mod info_param;
mod launch;
mod localise;
pub mod manifest;
mod socket;
mod sync;
mod webserver;

use crate::APP_HANDLE;
use crate::shared::{CATEGORIES, Category, config_dir, log_dir};
use launch::{INSTANCES, PLATFORM_NAME, PluginInstance};

use std::collections::HashMap;
use std::{fs, path};

use tauri::{AppHandle, Manager};

use anyhow::anyhow;
use once_cell::sync::Lazy;
use tokio::sync::RwLock;

pub static DEVICE_NAMESPACES: Lazy<RwLock<HashMap<String, String>>> = Lazy::new(|| RwLock::new(HashMap::new()));
const MICROSOFT_TEAMS_PLUGIN: &str = "com.microsoft.teams.sdPlugin";
const LOOPBACK_HOST: &str = "127.0.0.1";

fn find_available_port_base(mut base: u16) -> u16 {
	loop {
		let Some(webserver_port) = base.checked_add(2) else {
			base = 1024;
			continue;
		};
		let websocket_result = std::net::TcpListener::bind((LOOPBACK_HOST, base));
		let webserver_result = std::net::TcpListener::bind((LOOPBACK_HOST, webserver_port));
		if websocket_result.is_ok() && webserver_result.is_ok() {
			log::debug!("Using ports {} and {}", base, webserver_port);
			return base;
		}
		base = base.checked_add(1).unwrap_or(1024);
	}
}

pub static PORT_BASE: Lazy<u16> = Lazy::new(|| find_available_port_base(57116));

async fn register_actions(category_name: String, category_icon: Option<String>, actions: Vec<crate::shared::Action>) {
	let mut categories = CATEGORIES.write().await;
	if let Some(category) = categories.get_mut(&category_name) {
		for action in actions {
			if let Some(index) = category.actions.iter().position(|v| v.uuid == action.uuid) {
				category.actions.remove(index);
			}
			category.actions.push(action);
		}
		return;
	}

	if !actions.is_empty() {
		categories.insert(category_name, Category { icon: category_icon, actions });
	}
}

/// Initialise a plugin from a given directory.
pub async fn initialise_plugin(path: &path::Path) -> anyhow::Result<()> {
	let plugin_uuid = path.file_name().unwrap().to_str().unwrap();

	let mut manifest = manifest::read_manifest(path)?;
	localise::localise_manifest(&mut manifest, path, plugin_uuid);

	register_actions(manifest.category.clone(), manifest.category_icon.clone(), std::mem::take(&mut manifest.actions)).await;

	if let Some(namespace) = manifest.device_namespace.take() {
		DEVICE_NAMESPACES.write().await.insert(namespace, plugin_uuid.to_owned());
	}

	launch::launch(path, plugin_uuid, &manifest).await?;

	if let Some(applications) = manifest.applications_to_monitor.as_ref().and_then(|applications| applications.get(PLATFORM_NAME)) {
		crate::application_watcher::start_monitoring(plugin_uuid, applications).await;
	}

	Ok(())
}

pub async fn deactivate_plugin(app: &AppHandle, uuid: &str) -> Result<(), anyhow::Error> {
	{
		let mut namespaces = DEVICE_NAMESPACES.write().await;
		if let Some((namespace, _)) = namespaces.clone().iter().find(|(_, plugin)| uuid == **plugin) {
			namespaces.remove(namespace);
			drop(namespaces);
			let devices = crate::shared::DEVICES.iter().map(|v| v.key().to_owned()).filter(|id| &id[..2] == namespace).collect::<Vec<_>>();
			for device in devices {
				crate::events::inbound::devices::deregister_device("", crate::events::inbound::PayloadEvent { payload: device }).await?;
			}
			crate::events::frontend::update_devices().await;
		}
	}

	crate::application_watcher::stop_monitoring(uuid).await;

	if let Some(instance) = INSTANCES.lock().await.remove(uuid) {
		match instance {
			PluginInstance::Webview => {
				if let Some(window) = app.get_webview_window(&uuid.replace('.', "_")) {
					window.close()?;
					tokio::time::sleep(std::time::Duration::from_millis(10)).await;
				}
			}
			PluginInstance::Node(mut child) | PluginInstance::Wine(mut child) | PluginInstance::Native(mut child) => {
				child.kill()?;
				child.wait()?;
			}
		}
		Ok(())
	} else {
		Err(anyhow!("instance of plugin {} not found", uuid))
	}
}

/// Keep the Microsoft Teams plugin loaded only while the Teams process exists.
/// Closing its webview also cancels the plugin's internal reconnect timer.
pub async fn set_microsoft_teams_running(running: bool) -> Result<(), anyhow::Error> {
	let active = INSTANCES.lock().await.contains_key(MICROSOFT_TEAMS_PLUGIN);
	if running == active {
		return Ok(());
	}

	if running {
		let path = config_dir().join("plugins").join(MICROSOFT_TEAMS_PLUGIN);
		if !path.is_dir() {
			return Ok(());
		}
		log::info!("Microsoft Teams started; activating its plugin");
		initialise_plugin(&path).await
	} else {
		log::info!("Microsoft Teams stopped; deactivating its plugin and cancelling reconnects");
		deactivate_plugin(APP_HANDLE.get().unwrap(), MICROSOFT_TEAMS_PLUGIN).await
	}
}

#[cfg(windows)]
pub async fn deactivate_plugins() {
	let uuids = {
		let instances = INSTANCES.lock().await;
		instances.keys().cloned().collect::<Vec<_>>()
	};

	let app = APP_HANDLE.get().unwrap();
	for uuid in uuids {
		let _ = deactivate_plugin(app, &uuid).await;
	}
}

/// Initialise plugins from the plugins directory.
pub fn initialise_plugins() {
	tokio::spawn(socket::init_websocket_server());
	tokio::spawn(webserver::init_webserver(config_dir()));

	let plugin_dir = config_dir().join("plugins");
	let _ = fs::create_dir_all(&plugin_dir);
	let _ = fs::create_dir_all(log_dir().join("plugins"));

	sync::sync_builtin_plugins(&plugin_dir);
	sync::spawn_installed_plugins(&plugin_dir);
}

#[cfg(test)]
mod tests {
	use super::{LOOPBACK_HOST, find_available_port_base};

	#[test]
	fn port_selection_skips_a_loopback_asset_port_that_is_already_in_use() {
		let occupied_listener = std::net::TcpListener::bind((LOOPBACK_HOST, 0)).unwrap();
		let occupied_port = occupied_listener.local_addr().unwrap().port();
		let candidate_base = occupied_port - 2;

		assert_ne!(find_available_port_base(candidate_base), candidate_base);
	}
}
