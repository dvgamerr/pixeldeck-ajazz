pub(crate) mod devices;
mod misc;
mod property_inspector;
mod settings;
mod states;

use crate::{
	shared::ActionContext,
	store::profiles::{acquire_locks, get_instance},
};

use tokio_tungstenite::tungstenite::{Error, Message};

use log::warn;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "event")]
#[serde(rename_all = "camelCase")]
pub enum RegisterEvent {
	RegisterPlugin { uuid: String },
	RegisterPropertyInspector { uuid: String },
}

#[derive(Deserialize)]
pub struct ContextEvent<C = ActionContext> {
	pub context: C,
}

#[derive(Deserialize)]
pub struct PayloadEvent<T> {
	pub payload: T,
}

#[derive(Deserialize)]
pub struct ContextAndPayloadEvent<T, C = ActionContext> {
	pub context: C,
	pub payload: T,
}

#[derive(Deserialize)]
#[serde(tag = "event")]
#[serde(rename_all = "camelCase")]
pub enum InboundEventType {
	RegisterDevice(PayloadEvent<crate::shared::DeviceInfo>),
	DeregisterDevice(PayloadEvent<String>),
	RerenderImages(PayloadEvent<String>),
	KeyDown(PayloadEvent<devices::PressPayload>),
	KeyUp(PayloadEvent<devices::PressPayload>),
	EncoderChange(PayloadEvent<devices::TicksPayload>),
	EncoderDown(PayloadEvent<devices::PressPayload>),
	EncoderUp(PayloadEvent<devices::PressPayload>),
	SetSettings(ContextAndPayloadEvent<serde_json::Value>),
	GetSettings(ContextEvent),
	SetGlobalSettings(ContextAndPayloadEvent<serde_json::Value, String>),
	GetGlobalSettings(ContextEvent<String>),
	OpenUrl(PayloadEvent<misc::OpenUrlEvent>),
	LogMessage(PayloadEvent<misc::LogMessageEvent>),
	SetTitle(ContextAndPayloadEvent<states::SetTitlePayload>),
	SetImage(ContextAndPayloadEvent<states::SetImagePayload>),
	SetState(ContextAndPayloadEvent<states::SetStatePayload>),
	ShowAlert(ContextEvent),
	ShowOk(ContextEvent),
	SendToPropertyInspector(ContextAndPayloadEvent<serde_json::Value>),
	SendToPlugin(ContextAndPayloadEvent<serde_json::Value>),
	SwitchProfile(misc::SwitchProfileEvent),
	DeviceBrightness(misc::DeviceBrightnessEvent),
}

fn action_context(event: &InboundEventType) -> Option<&ActionContext> {
	match event {
		InboundEventType::SetSettings(event) => Some(&event.context),
		InboundEventType::GetSettings(event) => Some(&event.context),
		InboundEventType::SetTitle(event) => Some(&event.context),
		InboundEventType::SetImage(event) => Some(&event.context),
		InboundEventType::SetState(event) => Some(&event.context),
		InboundEventType::ShowAlert(event) => Some(&event.context),
		InboundEventType::ShowOk(event) => Some(&event.context),
		InboundEventType::SendToPropertyInspector(event) => Some(&event.context),
		_ => None,
	}
}

/// Checks that the plugin identified by `uuid` is allowed to send `event`.
async fn is_plugin_authorised(event: &InboundEventType, uuid: &str) -> bool {
	if let Some(context) = action_context(event) {
		return match get_instance(context, &acquire_locks().await).await {
			Ok(Some(instance)) => instance.action.plugin == uuid,
			_ => false,
		};
	}

	match event {
		InboundEventType::SetGlobalSettings(event) => event.context == uuid,
		InboundEventType::GetGlobalSettings(event) => event.context == uuid,
		InboundEventType::SwitchProfile(_) | InboundEventType::DeviceBrightness(_) => uuid == "com.amansprojects.starterpack.sdPlugin" || uuid == "opendeck_alternative_ajazz_implementation",
		_ => true,
	}
}

async fn dispatch_plugin_event(decoded: InboundEventType, uuid: &str) -> Result<(), anyhow::Error> {
	match decoded {
		InboundEventType::RegisterDevice(event) => devices::register_device(uuid, event).await,
		InboundEventType::DeregisterDevice(event) => devices::deregister_device(uuid, event).await,
		InboundEventType::RerenderImages(event) => devices::rerender_images(event).await,
		InboundEventType::KeyDown(event) => devices::key_down(event).await,
		InboundEventType::KeyUp(event) => devices::key_up(event).await,
		InboundEventType::EncoderChange(event) => devices::encoder_change(event).await,
		InboundEventType::EncoderDown(event) => devices::encoder_down(event).await,
		InboundEventType::EncoderUp(event) => devices::encoder_up(event).await,
		InboundEventType::SetSettings(event) => settings::set_settings(event, false).await,
		InboundEventType::GetSettings(event) => settings::get_settings(event, false).await,
		InboundEventType::SetGlobalSettings(event) => settings::set_global_settings(event, false).await,
		InboundEventType::GetGlobalSettings(event) => settings::get_global_settings(event, false).await,
		InboundEventType::OpenUrl(event) => misc::open_url(event).await,
		InboundEventType::LogMessage(event) => misc::log_message(Some(uuid), event).await,
		InboundEventType::SetTitle(event) => states::set_title(event).await,
		InboundEventType::SetImage(event) => states::set_image(event).await,
		InboundEventType::SetState(event) => states::set_state(event).await,
		InboundEventType::ShowAlert(event) => misc::show_alert(event).await,
		InboundEventType::ShowOk(event) => misc::show_ok(event).await,
		InboundEventType::SendToPropertyInspector(event) => property_inspector::send_to_property_inspector(event).await,
		InboundEventType::SendToPlugin(_) => Ok(()),
		InboundEventType::SwitchProfile(event) => misc::switch_profile(event).await,
		InboundEventType::DeviceBrightness(event) => misc::device_brightness(event).await,
	}
}

pub async fn process_incoming_message(data: Result<Message, Error>, uuid: &str, skip_auth: bool) {
	let Ok(Message::Text(text)) = data else {
		return;
	};
	let Ok(decoded) = serde_json::from_str::<InboundEventType>(&text) else {
		return;
	};

	if !(uuid.is_empty() && skip_auth) && !is_plugin_authorised(&decoded, uuid).await {
		return;
	}

	if let Err(error) = dispatch_plugin_event(decoded, uuid).await
		&& !error.to_string().contains("closed connection")
	{
		warn!("Failed to process incoming event from plugin: {}", error);
	}
}

pub async fn process_incoming_message_pi(data: Result<Message, Error>, uuid: &str) {
	if let Ok(Message::Text(text)) = data {
		let decoded: InboundEventType = match serde_json::from_str(&text) {
			Ok(event) => event,
			Err(_) => return,
		};

		let context = match &decoded {
			InboundEventType::SetSettings(event) => Some(event.context.to_string()),
			InboundEventType::GetSettings(event) => Some(event.context.to_string()),
			InboundEventType::SetGlobalSettings(event) => Some(event.context.clone()),
			InboundEventType::GetGlobalSettings(event) => Some(event.context.clone()),
			InboundEventType::SendToPlugin(event) => Some(event.context.to_string()),
			_ => None,
		};
		if context.is_some_and(|context| context != uuid) {
			return;
		}

		let result = match decoded {
			InboundEventType::SetSettings(event) => settings::set_settings(event, true).await,
			InboundEventType::GetSettings(event) => settings::get_settings(event, true).await,
			InboundEventType::SetGlobalSettings(event) => settings::set_global_settings(event, true).await,
			InboundEventType::GetGlobalSettings(event) => settings::get_global_settings(event, true).await,
			InboundEventType::OpenUrl(event) => misc::open_url(event).await,
			InboundEventType::LogMessage(event) => misc::log_message(None, event).await,
			InboundEventType::SendToPlugin(event) => property_inspector::send_to_plugin(event).await,
			_ => Ok(()),
		};
		if let Err(error) = result
			&& !error.to_string().contains("closed connection")
		{
			warn!("Failed to process incoming event from property inspector: {}", error);
		}
	}
}
