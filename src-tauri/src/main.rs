// Prevents additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ajazz;
mod application_watcher;
mod events;
mod plugins;
mod shared;
mod store;
mod window_state;
mod zip_extract;

mod built_info {
	include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

use events::frontend;
use shared::PRODUCT_NAME;

use once_cell::sync::OnceCell;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use tauri::{
	AppHandle, Builder, Manager, WindowEvent,
	menu::{IconMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
	tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
	webview::PageLoadEvent,
};
use tauri_plugin_log::{Target, TargetKind};

static APP_HANDLE: OnceCell<AppHandle> = OnceCell::new();
static MAIN_WINDOW_SHOWN: AtomicBool = AtomicBool::new(false);

fn show_window(app: &AppHandle) -> Result<(), tauri::Error> {
	#[cfg(target_os = "macos")]
	{
		use tauri::ActivationPolicy;
		let _ = app.set_activation_policy(ActivationPolicy::Regular);
	}

	let window = app.get_webview_window("main").ok_or_else(|| tauri::Error::WebviewNotFound)?;
	window.show().and_then(|_| window.set_focus())
}

fn hide_window(app: &AppHandle) -> Result<(), tauri::Error> {
	let window = app.get_webview_window("main").ok_or_else(|| tauri::Error::WebviewNotFound)?;
	window.hide()?;

	#[cfg(target_os = "macos")]
	{
		use tauri::ActivationPolicy;
		let _ = app.set_activation_policy(ActivationPolicy::Accessory);
	}

	Ok(())
}

fn handle_second_instance(app: &AppHandle, args: &[String]) {
	let find_flag = |flag: &str| args.iter().position(|x| x.to_lowercase().trim() == flag);

	if let Some(pos) = find_flag("--reload-plugin") {
		if let Some(id) = args.get(pos + 1) {
			tauri::async_runtime::spawn(frontend::plugins::reload_plugin(app.clone(), id.clone()));
		}
	} else if let Some(pos) = find_flag("--process-message") {
		if let Some(message) = args.get(pos + 1) {
			tauri::async_runtime::spawn(events::inbound::process_incoming_message(
				Ok(tokio_tungstenite::tungstenite::Message::Text(message.clone().into())),
				"",
				true,
			));
		}
	} else {
		let _ = show_window(app);
	}
}

fn migrate_legacy_config_dirs(app: &tauri::App) {
	for name in ["com.mistweaverco.opendeck-ajazz", "opendeck-ajazz"] {
		let old = app.path().config_dir().unwrap().join(name);
		if old.exists() {
			let _ = std::fs::rename(old, app.path().app_config_dir().unwrap());
		}
	}
}

/// Compares the application version against the one recorded in the settings.
/// Returns `Ok(false)` when the application must not continue starting up.
fn check_settings_version(app: &tauri::App, settings: &mut store::Store<store::Settings>) -> Result<bool, anyhow::Error> {
	use std::cmp::Ordering;
	use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

	let current_version = semver::Version::parse(built_info::PKG_VERSION)?;
	let settings_version = semver::Version::parse(&settings.value.version)?;
	let cmp = (current_version.major, current_version.minor).cmp(&(settings_version.major, settings_version.minor));
	match cmp {
		Ordering::Less => {
			app.get_webview_window("main").unwrap().close().unwrap();
			app.dialog()
				.message(format!(
					"A newer version of {PRODUCT_NAME} created configuration files on this device. This version is v{}; please upgrade to v{} or newer.",
					built_info::PKG_VERSION,
					settings.value.version
				))
				.title(format!("{PRODUCT_NAME} upgrade required"))
				.kind(MessageDialogKind::Error)
				.show(|_| APP_HANDLE.get().unwrap().exit(1));
			return Ok(false);
		}
		Ordering::Greater => {
			let old_version = settings.value.version.clone();
			settings.value.version = built_info::PKG_VERSION.to_owned();
			if old_version == "0.0.0" {
				settings.value.statistics = false;
			}
			settings.save()?;
			if old_version != "0.0.0" {
				app.dialog()
					.message(format!(
						r#"{PRODUCT_NAME} has been updated to v{}!
Every update brings features, bug fixes, and other improvements, which I spend my time implementing for free.

If you spent $125 on your hardware, please consider spending $10 on the software that makes it work.
You can donate to support development with just a few clicks on GitHub Sponsors.
If you have already donated, thank you so much for your support!"#,
						built_info::PKG_VERSION
					))
					.title(format!("{PRODUCT_NAME} has successfully been updated"))
					.kind(MessageDialogKind::Info)
					.show(|_| ());
			}
		}
		Ordering::Equal => {}
	}
	Ok(true)
}

fn init_statistics(app: &tauri::App, statistics: bool) -> Result<(), anyhow::Error> {
	use tauri_plugin_aptabase::{Builder, EventTracker, InitOptions};
	app.handle().plugin(
		Builder::new(if statistics { "A-SH-3841489320" } else { "" })
			.with_options(InitOptions {
				host: Some("https://aptabase.amankhanna.me".to_owned()),
				flush_interval: None,
			})
			.build(),
	)?;
	let _ = app.track_event("app_started", None);
	Ok(())
}

fn handle_tray_icon_event(icon: &tauri::tray::TrayIcon, event: TrayIconEvent) {
	let TrayIconEvent::Click { button, button_state, .. } = event else {
		return;
	};
	if button != MouseButton::Left || button_state != MouseButtonState::Down {
		return;
	}

	let app_handle = icon.app_handle();
	let window = app_handle.get_webview_window("main").unwrap();
	let _ = if window.is_visible().unwrap_or(false) { hide_window(app_handle) } else { show_window(app_handle) };
}

fn handle_tray_menu_event(app: &AppHandle, id: &str) {
	let _ = match id {
		"show" => show_window(app),
		"hide" => hide_window(app),
		"quit" => {
			app.exit(0);
			Ok(())
		}
		_ => Ok(()),
	};
}

fn init_tray(app: &tauri::App) -> Result<(), anyhow::Error> {
	let label = IconMenuItemBuilder::with_id("label", PRODUCT_NAME)
		.icon(app.default_window_icon().unwrap().clone())
		.enabled(false)
		.build(app)?;
	let show = MenuItemBuilder::with_id("show", "Show").build(app)?;
	let hide = MenuItemBuilder::with_id("hide", "Hide").build(app)?;
	let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
	let separator = PredefinedMenuItem::separator(app)?;
	let menu = MenuBuilder::new(app).items(&[&label, &separator, &show, &hide, &separator, &quit]).build()?;
	let _tray = TrayIconBuilder::new()
		.menu(&menu)
		.icon(app.default_window_icon().unwrap().clone())
		.show_menu_on_left_click(false)
		.on_tray_icon_event(handle_tray_icon_event)
		.on_menu_event(|app, event| handle_tray_menu_event(app, event.id().as_ref()))
		.build(app)?;
	Ok(())
}

async fn check_for_update() -> Result<(), anyhow::Error> {
	use tauri_plugin_dialog::DialogExt;

	let res = reqwest::Client::new()
		.get("https://api.github.com/repos/dvgamerr/pixeldeck-ajazz/releases/latest")
		.header("Accept", "application/vnd.github+json")
		.header("User-Agent", "pixeldeck-ajazz")
		.send()
		.await?
		.json::<serde_json::Value>()
		.await?;
	if !res.is_object() || res.get("tag_name").is_none() {
		return Ok(());
	}
	let tag_name = res.get("tag_name").unwrap().as_str().unwrap();
	if semver::Version::parse(built_info::PKG_VERSION)? >= semver::Version::parse(&tag_name[1..])? {
		return Ok(());
	}

	let app = APP_HANDLE.get().unwrap();
	app.dialog()
		.message(format!(
			"A new version of {PRODUCT_NAME}, {}, is available.\nUpdate description:\n\n{}",
			tag_name,
			res.get("body").map(|v| v.as_str().unwrap()).unwrap_or("No description").trim()
		))
		.title(format!("{PRODUCT_NAME} update available"))
		.show(|_| ());

	Ok(())
}

fn setup_app(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
	APP_HANDLE.set(app.handle().clone()).unwrap();
	if let Some(webview_window) = app.get_webview_window("main") {
		window_state::initialize(&webview_window.as_ref().window());
	}

	#[cfg(not(windows))]
	if std::env::args().any(|v| v == "--hide") {
		let _ = hide_window(app.handle());
	}

	migrate_legacy_config_dirs(app);

	let mut settings = store::get_settings()?;
	if !check_settings_version(app, &mut settings)? {
		return Ok(());
	}

	init_statistics(app, settings.value.statistics)?;

	tokio::spawn(async {
		loop {
			ajazz::initialise_devices().await;
			tokio::time::sleep(std::time::Duration::from_secs(10)).await;
		}
	});
	plugins::initialise_plugins();
	application_watcher::init_application_watcher();

	init_tray(app)?;

	#[cfg(any(target_os = "linux", all(debug_assertions, windows)))]
	{
		use tauri_plugin_deep_link::DeepLinkExt;
		let _ = app.deep_link().register_all();
	}

	if settings.value.updatecheck {
		tokio::spawn(async {
			if let Err(error) = check_for_update().await {
				log::warn!("Failed to update application: {error}");
			}
		});
	}

	Ok(())
}

fn handle_page_load(webview: &tauri::Webview, payload: &tauri::webview::PageLoadPayload<'_>) {
	if webview.label() != "main" || !matches!(payload.event(), PageLoadEvent::Finished) || std::env::args().any(|value| value == "--hide") {
		return;
	}
	if MAIN_WINDOW_SHOWN.swap(true, AtomicOrdering::SeqCst) {
		return;
	}

	let window = webview.window();
	tauri::async_runtime::spawn(async move {
		tokio::time::sleep(std::time::Duration::from_millis(80)).await;
		let _ = window.show();
		let _ = window.set_focus();
	});
}

fn handle_window_event(window: &tauri::Window, event: &WindowEvent) {
	if window.label() != "main" {
		return;
	}
	if matches!(event, WindowEvent::Moved(_) | WindowEvent::Resized(_)) {
		window_state::queue_save(window);
	}
	if let WindowEvent::CloseRequested { api, .. } = event {
		window_state::save_now(window);
		if let Ok(true) = store::get_settings().map(|store| store.value.background) {
			let _ = hide_window(window.app_handle());
			api.prevent_close();
		} else {
			window.app_handle().exit(0);
		}
	}
}

fn handle_run_event(app: &AppHandle, event: tauri::RunEvent) {
	if !matches!(event, tauri::RunEvent::Exit) {
		return;
	}
	if let Some(webview_window) = app.get_webview_window("main") {
		window_state::save_now(&webview_window.as_ref().window());
	}
	#[cfg(windows)]
	futures::executor::block_on(plugins::deactivate_plugins());
	tokio::spawn(ajazz::reset_devices());
	use tauri_plugin_aptabase::EventTracker;
	app.flush_events_blocking();
}

#[tokio::main]
async fn main() {
	log_panics::init();
	let _ = fix_path_env::fix();

	#[cfg(target_os = "linux")]
	// SAFETY: std::env::set_var can cause race conditions in multithreaded contexts. We have not spawned any other threads at this point.
	unsafe {
		std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
		std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
	}

	let app = match Builder::default()
		.plugin(tauri_plugin_single_instance::init(|app, args, _| handle_second_instance(app, &args)))
		.invoke_handler(tauri::generate_handler![
			frontend::restart,
			frontend::get_devices,
			frontend::devices::set_startup_image,
			frontend::devices::get_startup_image_project,
			frontend::devices::save_startup_image_project,
			frontend::get_port_base,
			frontend::get_categories,
			frontend::get_localisations,
			frontend::get_applications,
			frontend::get_application_profiles,
			frontend::set_application_profiles,
			frontend::instances::create_instance,
			frontend::instances::move_instance,
			frontend::instances::remove_instance,
			frontend::instances::set_state,
			frontend::instances::update_image,
			frontend::instances::update_images,
			frontend::profiles::get_profiles,
			frontend::profiles::get_selected_profile,
			frontend::profiles::reload_selected_profile,
			frontend::profiles::set_selected_profile,
			frontend::profiles::rename_profile,
			frontend::profiles::delete_profile,
			frontend::property_inspector::make_info,
			frontend::property_inspector::switch_property_inspector,
			frontend::property_inspector::open_url,
			frontend::plugins::list_plugins,
			frontend::plugins::install_plugin,
			frontend::plugins::remove_plugin,
			frontend::plugins::reload_plugin,
			frontend::plugins::show_settings_interface,
			frontend::settings::get_settings,
			frontend::settings::set_settings,
			frontend::settings::open_config_directory,
			frontend::settings::open_log_directory,
			frontend::settings::get_build_info
		])
		.setup(|app| setup_app(app))
		.on_page_load(handle_page_load)
		.plugin(
			tauri_plugin_log::Builder::default()
				.targets([Target::new(TargetKind::LogDir { file_name: None }), Target::new(TargetKind::Stdout)])
				.level(log::LevelFilter::Info)
				.level_for("pixeldeck-ajazz", log::LevelFilter::Trace)
				.build(),
		)
		.plugin(tauri_plugin_cors_fetch::init())
		.plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--hide"])))
		.plugin(tauri_plugin_dialog::init())
		.plugin(tauri_plugin_deep_link::init())
		.on_window_event(handle_window_event)
		.build(tauri::generate_context!())
	{
		Ok(app) => app,
		Err(error) => panic!("failed to build Tauri application: {}", error),
	};

	app.run(handle_run_event);
}
