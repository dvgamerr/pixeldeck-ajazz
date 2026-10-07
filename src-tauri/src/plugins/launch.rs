use super::{LOOPBACK_HOST, PORT_BASE, info_param, manifest};
use crate::APP_HANDLE;
use crate::built_info::TARGET;
use crate::shared::{is_flatpak, log_dir};
use crate::store::get_settings;

use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::{fs, path};

use anyhow::anyhow;
use once_cell::sync::Lazy;
use tokio::sync::Mutex;

pub(super) enum PluginInstance {
	Webview,
	Wine(Child),
	Native(Child),
	Node(Child),
}

pub(super) static INSTANCES: Lazy<Mutex<HashMap<String, PluginInstance>>> = Lazy::new(|| Mutex::new(HashMap::new()));

#[cfg(target_os = "windows")]
pub(super) const PLATFORM_NAME: &str = "windows";
#[cfg(target_os = "macos")]
pub(super) const PLATFORM_NAME: &str = "mac";
#[cfg(target_os = "linux")]
pub(super) const PLATFORM_NAME: &str = "linux";

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
		if os.platform == PLATFORM_NAME {
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
		_ => Err(anyhow!("unsupported on platform {}", PLATFORM_NAME)),
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

/// Launches a plugin according to the type of its code path.
pub(super) async fn launch(path: &path::Path, plugin_uuid: &str, manifest: &manifest::PluginManifest) -> anyhow::Result<()> {
	let (code_path, use_wine) = select_code_path(manifest)?;
	let version = manifest.version.clone();
	let port_string = PORT_BASE.to_string();
	let args = ["-port", port_string.as_str(), "-pluginUUID", plugin_uuid, "-registerEvent", "registerPlugin", "-info"];

	let code_path_lowercase = code_path.to_ascii_lowercase();
	if [".html", ".htm", ".xhtml"].iter().any(|extension| code_path_lowercase.ends_with(extension)) {
		launch_webview_plugin(path, plugin_uuid, &code_path, version).await
	} else if [".js", ".mjs", ".cjs"].iter().any(|extension| code_path_lowercase.ends_with(extension)) {
		launch_node_plugin(path, plugin_uuid, code_path, version, &args).await
	} else if use_wine {
		launch_wine_plugin(path, plugin_uuid, code_path, version, &args).await
	} else {
		launch_native_plugin(path, plugin_uuid, code_path, version, &args).await
	}
}

#[cfg(test)]
mod tests {
	use super::webview_plugin_initialization_script;

	#[test]
	fn webview_plugin_initialization_uses_the_requested_registration_details() {
		let script = webview_plugin_initialization_script(57_116, "com.microsoft.teams.sdPlugin", r#"{"application":"PixelDeck"}"#);

		assert!(script.contains(r#"connectOpenActionSocket(57116, "com.microsoft.teams.sdPlugin", "registerPlugin""#));
		assert!(script.contains(r#"connectElgatoStreamDeckSocket(57116, "com.microsoft.teams.sdPlugin", "registerPlugin""#));
		assert!(script.contains(r#"`{"application":"PixelDeck"}`"#));
		assert!(script.contains("setTimeout(opendeckInit, 10)"));
	}
}
