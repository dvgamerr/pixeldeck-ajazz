use crate::compat::*;

// Non-spec OpenDeck-specific protocols are used in this file.

#[derive(serde::Serialize)]
struct SwitchProfileEvent {
	event: &'static str,
	device: String,
	profile: String,
}

pub async fn switch(
	device: String,
	profile: String,
	outbound: &mut OutboundEventManager,
) -> EventHandlerResult {
	outbound
		.send_event(SwitchProfileEvent {
			event: "switchProfile",
			device,
			profile,
		})
		.await?;
	Ok(())
}

pub async fn key_up(event: KeyEvent, outbound: &mut OutboundEventManager) -> EventHandlerResult {
	switch(
		event
			.payload
			.settings
			.as_object()
			.and_then(|x| x.get("device"))
			.and_then(|x| x.as_str())
			.unwrap_or(&event.device)
			.to_owned(),
		event
			.payload
			.settings
			.as_object()
			.and_then(|x| x.get("profile"))
			.and_then(|x| x.as_str())
			.unwrap_or("Default")
			.to_owned(),
		outbound,
	)
	.await
}
