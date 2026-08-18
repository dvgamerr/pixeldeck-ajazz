use openaction::{Instance, OpenActionResult, get_instance};
use serde_json::Value as SettingsValue;

use crate::{
	model::{ActionKind, setting_string},
	platform, render,
};

pub async fn appear(
	_context: String,
	kind: ActionKind,
	settings: &SettingsValue,
	instance: &Instance,
) -> OpenActionResult<()> {
	if kind == ActionKind::PowerShell {
		instance
			.set_image(Some(render::powershell(settings, "idle")), None)
			.await?;
	}
	Ok(())
}

pub async fn settings_changed(
	context: String,
	kind: ActionKind,
	settings: &SettingsValue,
	instance: &Instance,
) -> OpenActionResult<()> {
	appear(context, kind, settings, instance).await
}

pub async fn press(
	context: String,
	kind: ActionKind,
	settings: SettingsValue,
	instance: &Instance,
) -> OpenActionResult<()> {
	if kind != ActionKind::PowerShell {
		return Ok(());
	}

	let script = setting_string(&settings, "script", "");
	instance
		.set_image(Some(render::powershell(&settings, "running")), None)
		.await?;
	tokio::spawn(async move {
		let result = platform::run_script(&script).await;
		let image = render::powershell(&settings, if result.is_ok() { "ok" } else { "error" });
		if let Err(error) = &result {
			log::warn!("PowerShell action failed for {context}: {error:#}");
		}
		if let Some(instance) = get_instance(context.clone()).await {
			let _ = instance.set_image(Some(image), None).await;
			if result.is_ok() {
				let _ = instance.show_ok().await;
			} else {
				let _ = instance.show_alert().await;
			}
		}
	});
	Ok(())
}
