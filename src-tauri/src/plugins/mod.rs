pub mod info_param;
pub mod manifest;
mod webserver;

use crate::APP_HANDLE;
use crate::built_info::TARGET;
use crate::shared::{CATEGORIES, Category, config_dir, convert_icon, is_flatpak, log_dir};
use crate::store::get_settings;

use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::{fs, path};

use tauri::{AppHandle, Manager};

use futures::StreamExt;
use tokio::net::{TcpListener, TcpStream};

use anyhow::anyhow;
use log::{error, warn};
use once_cell::sync::Lazy;
use tokio::sync::{Mutex, RwLock};

enum PluginInstance {
	Webview,
	Wine(Child),
	Native(Child),
	Node(Child),
}

pub static DEVICE_NAMESPACES: Lazy<RwLock<HashMap<String, String>>> = Lazy::new(|| RwLock::new(HashMap::new()));
static INSTANCES: Lazy<Mutex<HashMap<String, PluginInstance>>> = Lazy::new(|| Mutex::new(HashMap::new()));
const MICROSOFT_TEAMS_PLUGIN: &str = "com.microsoft.teams.sdPlugin";
const LOOPBACK_HOST: &str = "127.0.0.1";

fn should_sync_builtin_plugin(existing_version: &semver::Version, builtin_version: &semver::Version, development: bool) -> bool {
	development || existing_version < builtin_version
}

fn webview_plugin_initialization_script(port: u16, uuid: &str, info: &str) -> String {
	format!(
		r#"const opendeckInit = () => {{
			try {{
				if (typeof connectOpenActionSocket === "function") connectOpenActionSocket({port}, "{uuid}", "registerPlugin", `{info}`);
				else connectElgatoStreamDeckSocket({port}, "{uuid}", "registerPlugin", `{info}`);
			}} catch (e) {{
				setTimeout(opendeckInit, 10);
			}}
		}};
		opendeckInit();
		"#
	)
}

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

#[cfg(target_os = "windows")]
const PLATFORM: &str = "windows";
#[cfg(target_os = "macos")]
const PLATFORM: &str = "mac";
#[cfg(target_os = "linux")]
const PLATFORM: &str = "linux";

/// Maps fonts that are unavailable on every platform to bundled equivalents.
fn substitute_font_family(family: &str) -> Option<&'static str> {
	match family.to_lowercase().trim() {
		"arial" => Some("Liberation Sans"),
		"arial black" => Some("Archivo Black"),
		"comic sans ms" => Some("Comic Neue"),
		"courier" | "Courier New" => Some("Courier Prime"),
		"georgia" => Some("Tinos"),
		"impact" => Some("Anton"),
		"microsoft sans serif" | "Times New Roman" => Some("Liberation Serif"),
		"tahoma" | "Verdana" => Some("Open Sans"),
		"trebuchet ms" => Some("Fira Sans"),
		_ => None,
	}
}

/// Resolves icon, property inspector and font paths of the manifest's actions relative to the plugin directory.
fn localise_manifest(manifest: &mut manifest::PluginManifest, path: &path::Path, plugin_uuid: &str) {
	if let Some(icon) = manifest.category_icon.take() {
		let category_icon_path = path.join(icon);
		manifest.category_icon = Some(convert_icon(category_icon_path.to_string_lossy().to_string()));
	}

	for action in &mut manifest.actions {
		plugin_uuid.clone_into(&mut action.plugin);

		let action_icon_path = path.join(action.icon.clone());
		action.icon = convert_icon(action_icon_path.to_str().unwrap().to_owned());

		if !action.property_inspector.is_empty() {
			action.property_inspector = path.join(&action.property_inspector).to_string_lossy().to_string();
		} else if let Some(ref property_inspector) = manifest.property_inspector_path {
			action.property_inspector = path.join(property_inspector).to_string_lossy().to_string();
		}

		for state in &mut action.states {
			if state.image == "actionDefaultImage" {
				state.image.clone_from(&action.icon);
			} else {
				let state_icon = path.join(state.image.clone());
				state.image = convert_icon(state_icon.to_str().unwrap().to_owned());
			}

			if let Some(family) = substitute_font_family(&state.family) {
				family.clone_into(&mut state.family);
			}
		}
	}
}

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

fn platform_code_path(manifest: &manifest::PluginManifest) -> Option<String> {
	#[cfg(target_os = "windows")]
	let specific = manifest.code_path_windows.clone();
	#[cfg(target_os = "macos")]
	let specific = manifest.code_path_macos.clone();
	#[cfg(target_os = "linux")]
	let specific = manifest.code_path_linux.clone();

	let code_path = specific.or_else(|| manifest.code_path.clone());
	manifest.code_paths.as_ref().and_then(|p| p.get(TARGET).cloned()).or(code_path)
}

/// Determines the executable of a plugin and whether it has to run through Wine,
/// based on its supported operating systems and the current operating system.
fn select_code_path(manifest: &manifest::PluginManifest) -> anyhow::Result<(String, bool)> {
	let mut code_path = manifest.code_path.clone();
	let mut use_wine = false;
	let mut supported = false;

	for os in &manifest.os {
		if os.platform == PLATFORM {
			code_path = platform_code_path(manifest);
			use_wine = false;
			supported = true;
			break;
		} else if os.platform == "windows" {
			use_wine = true;
			supported = true;
		}
	}

	if code_path.is_none() && use_wine {
		code_path.clone_from(&manifest.code_path_windows);
	}

	match code_path {
		Some(code_path) if supported => Ok((code_path, use_wine)),
		_ => Err(anyhow!("unsupported on platform {}", PLATFORM)),
	}
}

/// Spawns a plugin process with its output redirected to the plugin's log file.
fn spawn_plugin_process(command: &mut Command, plugin_uuid: &str, args: &[&str], info: &impl serde::Serialize) -> anyhow::Result<Child> {
	let log_file = fs::File::create(log_dir().join("plugins").join(format!("{plugin_uuid}.log")))?;
	command
		.args(args)
		.arg(serde_json::to_string(info)?)
		.stdout(Stdio::from(log_file.try_clone()?))
		.stderr(Stdio::from(log_file));

	#[cfg(target_os = "windows")]
	{
		use std::os::windows::process::CommandExt;
		command.creation_flags(0x08000000);
	}

	Ok(command.spawn()?)
}

async fn launch_webview_plugin(path: &path::Path, plugin_uuid: &str, code_path: &str, version: String) -> anyhow::Result<()> {
	let url = format!("http://{}:{}/", LOOPBACK_HOST, *PORT_BASE + 2) + path.join(code_path).to_str().unwrap();
	let info = info_param::make_info(plugin_uuid.to_owned(), version, false).await;
	let initialization_script = webview_plugin_initialization_script(*PORT_BASE, plugin_uuid, &serde_json::to_string(&info)?);
	let plugin_uuid_for_log = plugin_uuid.to_owned();
	let window = tauri::WebviewWindowBuilder::new(APP_HANDLE.get().unwrap(), plugin_uuid.replace('.', "_"), tauri::WebviewUrl::External(url.parse()?))
		.title(plugin_uuid)
		.visible(false)
		.on_page_load(move |window, payload| {
			if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished)
				&& let Err(error) = window.eval(initialization_script.clone())
			{
				log::error!("Failed to initialise webview plugin {plugin_uuid_for_log}: {error}");
			}
		})
		.build()?;

	if let Ok(store) = get_settings()
		&& store.value.developer
	{
		let _ = window.show();
		window.open_devtools();
	}

	INSTANCES.lock().await.insert(plugin_uuid.to_owned(), PluginInstance::Webview);
	Ok(())
}

async fn launch_node_plugin(path: &path::Path, plugin_uuid: &str, code_path: String, version: String, args: &[&str]) -> anyhow::Result<()> {
	// Check for Node.js installation and version in one go.
	let command = if is_flatpak() { "flatpak-spawn" } else { "node" };
	let extra_args = if is_flatpak() { vec!["--host", "node"] } else { vec![] };
	let version_output = Command::new(command).args(&extra_args).arg("--version").output();
	if version_output.is_err() || String::from_utf8(version_output.unwrap().stdout).unwrap().trim() < "v20.0.0" {
		return Err(anyhow!("Node.js version 20.0.0 or higher is required"));
	}

	let info = info_param::make_info(plugin_uuid.to_owned(), version, true).await;
	let mut command = Command::new(command);
	command.current_dir(path).args(extra_args).arg(code_path);
	let child = spawn_plugin_process(&mut command, plugin_uuid, args, &info)?;

	INSTANCES.lock().await.insert(plugin_uuid.to_owned(), PluginInstance::Node(child));
	Ok(())
}

async fn launch_wine_plugin(path: &path::Path, plugin_uuid: &str, code_path: String, version: String, args: &[&str]) -> anyhow::Result<()> {
	let command = if is_flatpak() { "flatpak-spawn" } else { "wine" };
	let extra_args = if is_flatpak() { vec!["--host", "wine"] } else { vec![] };
	let result = Command::new(command)
		.args(&extra_args)
		.arg("--version")
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.spawn()
		.and_then(|mut child| child.wait())
		.map(|status| status.success());
	if !matches!(result, Ok(true)) {
		return Err(anyhow!("failed to detect an installation of Wine"));
	}

	let info = info_param::make_info(plugin_uuid.to_owned(), version, true).await;
	let mut command = Command::new(command);
	command.current_dir(path).args(extra_args).arg(code_path);
	if get_settings()?.value.separatewine {
		command.env("WINEPREFIX", path.join("wineprefix").to_string_lossy().to_string());
	} else {
		let _ = fs::remove_dir_all(path.join("wineprefix"));
	}
	let child = spawn_plugin_process(&mut command, plugin_uuid, args, &info)?;

	INSTANCES.lock().await.insert(plugin_uuid.to_owned(), PluginInstance::Wine(child));
	Ok(())
}

async fn launch_native_plugin(path: &path::Path, plugin_uuid: &str, code_path: String, version: String, args: &[&str]) -> anyhow::Result<()> {
	let info = info_param::make_info(plugin_uuid.to_owned(), version, false).await;

	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		fs::set_permissions(path.join(&code_path), fs::Permissions::from_mode(0o755))?;
	}

	let mut command = Command::new(path.join(code_path));
	command.current_dir(path);
	let child = spawn_plugin_process(&mut command, plugin_uuid, args, &info)?;

	INSTANCES.lock().await.insert(plugin_uuid.to_owned(), PluginInstance::Native(child));
	Ok(())
}

/// Initialise a plugin from a given directory.
pub async fn initialise_plugin(path: &path::Path) -> anyhow::Result<()> {
	let plugin_uuid = path.file_name().unwrap().to_str().unwrap();

	let mut manifest = manifest::read_manifest(path)?;
	localise_manifest(&mut manifest, path, plugin_uuid);

	register_actions(manifest.category.clone(), manifest.category_icon.clone(), std::mem::take(&mut manifest.actions)).await;

	if let Some(namespace) = manifest.device_namespace.take() {
		DEVICE_NAMESPACES.write().await.insert(namespace, plugin_uuid.to_owned());
	}

	let (code_path, use_wine) = select_code_path(&manifest)?;
	let version = manifest.version.clone();
	let port_string = PORT_BASE.to_string();
	let args = ["-port", port_string.as_str(), "-pluginUUID", plugin_uuid, "-registerEvent", "registerPlugin", "-info"];

	let code_path_lowercase = code_path.to_ascii_lowercase();
	if [".html", ".htm", ".xhtml"].iter().any(|extension| code_path_lowercase.ends_with(extension)) {
		launch_webview_plugin(path, plugin_uuid, &code_path, version).await?;
	} else if [".js", ".mjs", ".cjs"].iter().any(|extension| code_path_lowercase.ends_with(extension)) {
		launch_node_plugin(path, plugin_uuid, code_path, version, &args).await?;
	} else if use_wine {
		launch_wine_plugin(path, plugin_uuid, code_path, version, &args).await?;
	} else {
		launch_native_plugin(path, plugin_uuid, code_path, version, &args).await?;
	}

	if let Some(applications) = manifest.applications_to_monitor.as_ref().and_then(|applications| applications.get(PLATFORM)) {
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

fn read_manifest_version(plugin_dir: &path::Path) -> Result<semver::Version, anyhow::Error> {
	let manifest = serde_json::from_slice::<manifest::PluginManifest>(&fs::read(plugin_dir.join("manifest.json"))?)?;
	Ok(semver::Version::parse(&manifest.version)?)
}

fn builtin_plugin_needs_sync(existing_path: &path::Path, builtin_version: &semver::Version) -> bool {
	read_manifest_version(existing_path)
		.map(|existing_version| should_sync_builtin_plugin(&existing_version, builtin_version, cfg!(debug_assertions)))
		.unwrap_or(true)
}

/// Replaces an installed plugin with the bundled copy, restoring the previous one if copying fails.
fn replace_installed_plugin(builtin_path: &path::Path, existing_path: &path::Path) -> Result<(), anyhow::Error> {
	let old_path = existing_path.with_extension("old");
	if existing_path.exists() {
		fs::rename(existing_path, &old_path)?;
	}
	if crate::shared::copy_dir(builtin_path, existing_path).is_err() && old_path.exists() {
		fs::rename(&old_path, existing_path)?;
	}
	let _ = fs::remove_dir_all(&old_path);
	Ok(())
}

fn sync_builtin_plugin(entry: &fs::DirEntry, plugin_dir: &path::Path) -> Result<(), anyhow::Error> {
	let builtin_version = read_manifest_version(&entry.path())?;
	let existing_path = plugin_dir.join(entry.file_name());
	if builtin_plugin_needs_sync(&existing_path, &builtin_version) {
		replace_installed_plugin(&entry.path(), &existing_path)?;
	}
	Ok(())
}

/// Copies bundled plugins into the plugins directory when they are missing or outdated.
fn sync_builtin_plugins(plugin_dir: &path::Path) {
	let Ok(Ok(entries)) = APP_HANDLE.get().unwrap().path().resolve("plugins", tauri::path::BaseDirectory::Resource).map(fs::read_dir) else {
		return;
	};
	for entry in entries.flatten() {
		if let Err(error) = sync_builtin_plugin(&entry, plugin_dir) {
			error!("Failed to upgrade builtin plugin {}: {}", entry.file_name().to_string_lossy(), error);
		}
	}
}

/// Initialises every plugin directory found in the plugins folder.
fn spawn_installed_plugins(plugin_dir: &path::Path) {
	let entries = match fs::read_dir(plugin_dir) {
		Ok(p) => p,
		Err(error) => {
			error!("Failed to read plugins directory at {}: {}", plugin_dir.display(), error);
			panic!()
		}
	};

	for entry in entries {
		let entry = match entry {
			Ok(entry) => entry,
			Err(error) => {
				warn!("Failed to read entry of plugins directory: {}", error);
				continue;
			}
		};
		let path = match entry.metadata().unwrap().is_symlink() {
			true => fs::read_link(entry.path()).unwrap(),
			false => entry.path(),
		};
		if !fs::metadata(&path).unwrap().is_dir() {
			continue;
		}
		tokio::spawn(async move {
			if let Err(error) = initialise_plugin(&path).await {
				warn!("Failed to initialise plugin at {}: {:#}", path.display(), error);
			}
		});
	}
}

/// Initialise plugins from the plugins directory.
pub fn initialise_plugins() {
	tokio::spawn(init_websocket_server());
	tokio::spawn(webserver::init_webserver(config_dir()));

	let plugin_dir = config_dir().join("plugins");
	let _ = fs::create_dir_all(&plugin_dir);
	let _ = fs::create_dir_all(log_dir().join("plugins"));

	sync_builtin_plugins(&plugin_dir);
	spawn_installed_plugins(&plugin_dir);
}

/// Start the WebSocket server that plugins communicate with.
async fn init_websocket_server() {
	let listener = match TcpListener::bind((LOOPBACK_HOST, *PORT_BASE)).await {
		Ok(listener) => listener,
		Err(error) => {
			error!("Failed to bind plugin WebSocket server to socket: {}", error);
			return;
		}
	};

	#[cfg(windows)]
	{
		use std::os::windows::io::AsRawSocket;
		use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};

		unsafe { SetHandleInformation(listener.as_raw_socket() as _, HANDLE_FLAG_INHERIT, 0) };
	}

	while let Ok((stream, _)) = listener.accept().await {
		accept_connection(stream).await;
	}
}

/// Handle incoming data from a WebSocket connection.
async fn accept_connection(stream: TcpStream) {
	let mut socket = match tokio_tungstenite::accept_async(stream).await {
		Ok(socket) => socket,
		Err(error) => {
			warn!("Failed to complete WebSocket handshake: {}", error);
			return;
		}
	};

	let Ok(register_event) = socket.next().await.unwrap() else {
		return;
	};
	match serde_json::from_str(&register_event.clone().into_text().unwrap()) {
		Ok(event) => crate::events::register_plugin(event, socket).await,
		Err(_) => {
			let _ = crate::events::inbound::process_incoming_message(Ok(register_event), "", false).await;
		}
	}
}

#[cfg(test)]
mod tests {
	use super::{LOOPBACK_HOST, find_available_port_base, should_sync_builtin_plugin, webview_plugin_initialization_script};
	use semver::Version;

	#[test]
	fn development_builds_always_sync_builtin_plugins() {
		let builtin = Version::new(2, 12, 3);

		assert!(should_sync_builtin_plugin(&Version::new(2, 12, 3), &builtin, true));
		assert!(should_sync_builtin_plugin(&Version::new(3, 0, 0), &builtin, true));
	}

	#[test]
	fn release_builds_only_upgrade_older_builtin_plugins() {
		let builtin = Version::new(2, 12, 3);

		assert!(should_sync_builtin_plugin(&Version::new(2, 12, 2), &builtin, false));
		assert!(!should_sync_builtin_plugin(&Version::new(2, 12, 3), &builtin, false));
		assert!(!should_sync_builtin_plugin(&Version::new(3, 0, 0), &builtin, false));
	}

	#[test]
	fn webview_plugin_initialization_uses_the_requested_registration_details() {
		let script = webview_plugin_initialization_script(57_116, "com.microsoft.teams.sdPlugin", r#"{"application":"PixelDeck"}"#);

		assert!(script.contains(r#"connectOpenActionSocket(57116, "com.microsoft.teams.sdPlugin", "registerPlugin""#));
		assert!(script.contains(r#"connectElgatoStreamDeckSocket(57116, "com.microsoft.teams.sdPlugin", "registerPlugin""#));
		assert!(script.contains(r#"`{"application":"PixelDeck"}`"#));
		assert!(script.contains("setTimeout(opendeckInit, 10)"));
	}

	#[test]
	fn port_selection_skips_a_loopback_asset_port_that_is_already_in_use() {
		let occupied_listener = std::net::TcpListener::bind((LOOPBACK_HOST, 0)).unwrap();
		let occupied_port = occupied_listener.local_addr().unwrap().port();
		let candidate_base = occupied_port - 2;

		assert_ne!(find_available_port_base(candidate_base), candidate_base);
	}
}
