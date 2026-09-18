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
		.plugin(tauri_plugin_single_instance::init(|app, args, _| {
			if let Some(pos) = args.iter().position(|x| x.to_lowercase().trim() == "--reload-plugin") {
				if args.len() > pos + 1 {
					tauri::async_runtime::spawn(frontend::plugins::reload_plugin(app.clone(), args[pos + 1].clone()));
				}
			} else if let Some(pos) = args.iter().position(|x| x.to_lowercase().trim() == "--process-message") {
				if args.len() > pos + 1 {
					tauri::async_runtime::spawn(events::inbound::process_incoming_message(
						Ok(tokio_tungstenite::tungstenite::Message::Text(args[pos + 1].clone().into())),
						"",
						true,
					));
				}
			} else {
				let _ = show_window(app);
			}
		}))
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
		.setup(|app| {
			APP_HANDLE.set(app.handle().clone()).unwrap();
			if let Some(webview_window) = app.get_webview_window("main") {
				window_state::initialize(&webview_window.as_ref().window());
			}

			#[cfg(not(windows))]
			if std::env::args().any(|v| v == "--hide") {
				let _ = hide_window(app.handle());
			}

			let old = app.path().config_dir().unwrap().join("com.mistweaverco.opendeck-ajazz");
			if old.exists() {
				let _ = std::fs::rename(old, app.path().app_config_dir().unwrap());
			}

			let old = app.path().config_dir().unwrap().join("opendeck-ajazz");
			if old.exists() {
				let _ = std::fs::rename(old, app.path().app_config_dir().unwrap());
			}

			let mut settings = store::get_settings()?;
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
					return Ok(());
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
				_ => {}
			}

			use tauri_plugin_aptabase::{Builder, EventTracker, InitOptions};
			app.handle().plugin(
				Builder::new(if settings.value.statistics { "A-SH-3841489320" } else { "" })
					.with_options(InitOptions {
						host: Some("https://aptabase.amankhanna.me".to_owned()),
						flush_interval: None,
					})
					.build(),
			)?;
			let _ = app.track_event("app_started", None);

			tokio::spawn(async {
				loop {
					ajazz::initialise_devices().await;
					tokio::time::sleep(std::time::Duration::from_secs(10)).await;
				}
			});
			plugins::initialise_plugins();
			application_watcher::init_application_watcher();

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
				.on_tray_icon_event(move |icon, event| {
					if let TrayIconEvent::Click { button, button_state, .. } = event {
						if button != MouseButton::Left || button_state != MouseButtonState::Down {
							return;
						}

						let app_handle = icon.app_handle();
						let window = app_handle.get_webview_window("main").unwrap();
						let _ = if window.is_visible().unwrap_or(false) { hide_window(app_handle) } else { show_window(app_handle) };
					}
				})
				.on_menu_event(move |app, event| {
					let _ = match event.id().as_ref() {
						"show" => show_window(app),
						"hide" => hide_window(app),
						"quit" => {
							app.exit(0);
							Ok(())
						}
						_ => Ok(()),
					};
				})
				.build(app)?;

			#[cfg(any(target_os = "linux", all(debug_assertions, windows)))]
			{
				use tauri_plugin_deep_link::DeepLinkExt;
				let _ = app.deep_link().register_all();
			}

			async fn update() -> Result<(), anyhow::Error> {
				let res = reqwest::Client::new()
					.get("https://api.github.com/repos/dvgamerr/pixeldeck-ajazz/releases/latest")
					.header("Accept", "application/vnd.github+json")
					.header("User-Agent", "pixeldeck-ajazz")
					.send()
					.await?
					.json::<serde_json::Value>()
					.await?;
				// A release payload we cannot read is not worth reporting: the check is
				// best-effort and the application works fine without it.
				let Some(tag_name) = res.get("tag_name").and_then(serde_json::Value::as_str) else {
					return Ok(());
				};
				let Ok(latest) = semver::Version::parse(tag_name.trim_start_matches('v')) else {
					return Ok(());
				};
				if semver::Version::parse(built_info::PKG_VERSION)?.cmp(&latest) == Ordering::Less {
					let app = APP_HANDLE.get().unwrap();
					app.dialog()
						.message(format!(
							"A new version of {PRODUCT_NAME}, {}, is available.\nUpdate description:\n\n{}",
							tag_name,
							res.get("body").and_then(serde_json::Value::as_str).unwrap_or("No description").trim()
						))
						.title(format!("{PRODUCT_NAME} update available"))
						.show(|_| ());
				}

				Ok(())
			}

			if settings.value.updatecheck {
				tokio::spawn(async {
					if let Err(error) = update().await {
						// Usually just a machine that is offline; skip the check silently.
						log::debug!("Update check skipped: {error}");
					}
				});
			}

			Ok(())
		})
		.on_page_load(|webview, payload| {
			if webview.label() == "main"
				&& matches!(payload.event(), PageLoadEvent::Finished)
				&& !std::env::args().any(|value| value == "--hide")
				&& !MAIN_WINDOW_SHOWN.swap(true, AtomicOrdering::SeqCst)
			{
				let window = webview.window();
				tauri::async_runtime::spawn(async move {
					tokio::time::sleep(std::time::Duration::from_millis(80)).await;
					let _ = window.show();
					let _ = window.set_focus();
				});
			}
		})
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
		.on_window_event(|window, event| {
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
		})
		.build(tauri::generate_context!())
	{
		Ok(app) => app,
		Err(error) => panic!("failed to build Tauri application: {}", error),
	};

	app.run(|app, event| {
		if let tauri::RunEvent::Exit = event {
			if let Some(webview_window) = app.get_webview_window("main") {
				window_state::save_now(&webview_window.as_ref().window());
			}
			#[cfg(windows)]
			futures::executor::block_on(plugins::deactivate_plugins());
			tokio::spawn(ajazz::reset_devices());
			use tauri_plugin_aptabase::EventTracker;
			app.flush_events_blocking();
		}
	});
}
