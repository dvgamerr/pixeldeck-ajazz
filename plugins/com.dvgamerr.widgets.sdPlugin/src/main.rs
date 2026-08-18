mod fetch;
mod local;
mod model;
mod pixel;
mod platform;
mod render;
mod runtime;

use model::ActionKind;
use openaction::*;
use serde_json::Value;

async fn key_up(instance: &Instance, kind: ActionKind, settings: &Value) -> OpenActionResult<()> {
	match kind {
		ActionKind::Gold => {
			open_url("https://www.tradingview.com/symbols/XAUUSD/".to_owned()).await?;
		}
		ActionKind::Currency | ActionKind::Stock | ActionKind::AirQuality | ActionKind::Weather => {
			runtime::refresh(&instance.instance_id)
		}
		ActionKind::PowerShell => {
			local::press(
				instance.instance_id.clone(),
				kind,
				settings.clone(),
				instance,
			)
			.await?;
		}
		ActionKind::WorkHours => {}
	}
	Ok(())
}

async fn will_appear(
	instance: &Instance,
	kind: ActionKind,
	settings: &Value,
) -> OpenActionResult<()> {
	if kind.scheduled() {
		runtime::appear(
			instance.instance_id.clone(),
			kind,
			settings.clone(),
			instance,
		)
		.await
	} else {
		local::appear(instance.instance_id.clone(), kind, settings, instance).await
	}
}

async fn will_disappear(instance: &Instance, kind: ActionKind) -> OpenActionResult<()> {
	if kind.scheduled() {
		runtime::disappear(&instance.instance_id);
	}
	Ok(())
}

async fn did_receive_settings(
	instance: &Instance,
	kind: ActionKind,
	settings: &Value,
) -> OpenActionResult<()> {
	if kind.scheduled() {
		if let Some(image) = runtime::update(&instance.instance_id, settings.clone()) {
			instance.set_image(Some(image), None).await?;
		}
		Ok(())
	} else {
		local::settings_changed(instance.instance_id.clone(), kind, settings, instance).await
	}
}

macro_rules! widget_action {
	($name:ident, $uuid:literal, $kind:expr) => {
		struct $name;

		#[async_trait]
		impl Action for $name {
			const UUID: ActionUuid = $uuid;
			type Settings = Value;

			async fn key_up(&self, instance: &Instance, settings: &Value) -> OpenActionResult<()> {
				key_up(instance, $kind, settings).await
			}

			async fn will_appear(
				&self,
				instance: &Instance,
				settings: &Value,
			) -> OpenActionResult<()> {
				will_appear(instance, $kind, settings).await
			}

			async fn will_disappear(
				&self,
				instance: &Instance,
				_settings: &Value,
			) -> OpenActionResult<()> {
				will_disappear(instance, $kind).await
			}

			async fn did_receive_settings(
				&self,
				instance: &Instance,
				settings: &Value,
			) -> OpenActionResult<()> {
				did_receive_settings(instance, $kind, settings).await
			}
		}
	};
}

widget_action!(
	GoldAction,
	"com.dvgamerr.widgets.gold-price",
	ActionKind::Gold
);
widget_action!(
	CurrencyAction,
	"com.dvgamerr.widgets.currency-rate",
	ActionKind::Currency
);
widget_action!(
	StockAction,
	"com.dvgamerr.widgets.stock-price",
	ActionKind::Stock
);
widget_action!(
	AirQualityAction,
	"com.dvgamerr.widgets.air-quality",
	ActionKind::AirQuality
);
widget_action!(
	PowerShellAction,
	"com.dvgamerr.widgets.powershell",
	ActionKind::PowerShell
);
widget_action!(
	WeatherAction,
	"com.dvgamerr.widgets.weather",
	ActionKind::Weather
);
widget_action!(
	WorkHoursAction,
	"com.dvgamerr.widgets.work-hours",
	ActionKind::WorkHours
);

#[tokio::main]
async fn main() -> OpenActionResult<()> {
	let _ = simplelog::TermLogger::init(
		simplelog::LevelFilter::Info,
		simplelog::Config::default(),
		simplelog::TerminalMode::Stdout,
		simplelog::ColorChoice::Never,
	);

	register_action(GoldAction).await;
	register_action(CurrencyAction).await;
	register_action(StockAction).await;
	register_action(AirQualityAction).await;
	register_action(PowerShellAction).await;
	register_action(WeatherAction).await;
	register_action(WorkHoursAction).await;
	run(std::env::args().collect()).await
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
