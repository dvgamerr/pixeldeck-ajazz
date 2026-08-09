mod fetch;
mod local;
mod model;
mod pixel;
mod platform;
mod render;
mod runtime;

use model::ActionKind;
use openaction::*;

struct GlobalEventHandler;
impl openaction::GlobalEventHandler for GlobalEventHandler {}

struct ActionEventHandler;
impl openaction::ActionEventHandler for ActionEventHandler {
	async fn key_up(
		&self,
		event: KeyEvent,
		outbound: &mut OutboundEventManager,
	) -> EventHandlerResult {
		let Some(kind) = ActionKind::from_uuid(&event.action) else {
			return Ok(());
		};
		match kind {
			ActionKind::Gold => {
				outbound
					.open_url("https://www.tradingview.com/symbols/XAUUSD/".to_owned())
					.await?;
			}
			ActionKind::Currency
			| ActionKind::Stock
			| ActionKind::AirQuality
			| ActionKind::Weather => runtime::refresh(&event.context),
			ActionKind::PowerShell => {
				local::press(event.context, kind, event.payload.settings, outbound).await?;
			}
			ActionKind::WorkHours => {}
		}
		Ok(())
	}

	async fn will_appear(
		&self,
		event: AppearEvent,
		outbound: &mut OutboundEventManager,
	) -> EventHandlerResult {
		let Some(kind) = ActionKind::from_uuid(&event.action) else {
			return Ok(());
		};
		if kind.scheduled() {
			runtime::appear(event.context, kind, event.payload.settings, outbound).await
		} else {
			local::appear(event.context, kind, &event.payload.settings, outbound).await
		}
	}

	async fn will_disappear(
		&self,
		event: AppearEvent,
		_outbound: &mut OutboundEventManager,
	) -> EventHandlerResult {
		if ActionKind::from_uuid(&event.action).is_some_and(ActionKind::scheduled) {
			runtime::disappear(&event.context);
		}
		Ok(())
	}

	async fn did_receive_settings(
		&self,
		event: DidReceiveSettingsEvent,
		outbound: &mut OutboundEventManager,
	) -> EventHandlerResult {
		let Some(kind) = ActionKind::from_uuid(&event.action) else {
			return Ok(());
		};
		if kind.scheduled() {
			if let Some(image) = runtime::update(&event.context, event.payload.settings) {
				outbound.set_image(event.context, Some(image), None).await?;
			}
			Ok(())
		} else {
			local::settings_changed(event.context, kind, &event.payload.settings, outbound).await
		}
	}
}

#[tokio::main]
async fn main() {
	let _ = simplelog::TermLogger::init(
		simplelog::LevelFilter::Info,
		simplelog::Config::default(),
		simplelog::TerminalMode::Stdout,
		simplelog::ColorChoice::Never,
	);

	if let Err(error) = init_plugin(GlobalEventHandler, ActionEventHandler).await {
		log::error!("Failed to initialise PixelDeck Widgets: {error}");
	}
}

#[cfg(test)]
mod tests {
	use super::ActionKind;
	use serde_json::Value;
	use std::collections::HashSet;

	#[test]
	fn every_manifest_action_has_a_native_handler() {
		let manifest: Value =
			serde_json::from_str(include_str!("../assets/manifest.json")).unwrap();
		let actions = manifest["Actions"].as_array().unwrap();
		let mut identifiers = HashSet::new();
		assert_eq!(actions.len(), 7);
		for action in actions {
			let identifier = action["UUID"].as_str().unwrap();
			assert!(identifiers.insert(identifier));
			assert!(
				ActionKind::from_uuid(identifier).is_some(),
				"missing handler for {identifier}"
			);
			assert_eq!(
				action["PropertyInspectorPath"],
				"propertyInspector/widget.html"
			);
		}
	}
}
