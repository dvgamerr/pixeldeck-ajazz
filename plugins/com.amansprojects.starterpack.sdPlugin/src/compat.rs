use std::sync::LazyLock;

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::Mutex;

pub type SettingsValue = Value;
pub type EventHandlerResult = Result<()>;

#[derive(Clone)]
pub struct EventPayload {
	pub settings: SettingsValue,
	pub ticks: i16,
}

#[derive(Clone)]
pub struct PluginEvent {
	pub action: String,
	pub context: String,
	pub device: String,
	pub payload: EventPayload,
}

pub type KeyEvent = PluginEvent;
pub type DialPressEvent = PluginEvent;
pub type DialRotateEvent = PluginEvent;

pub trait ActionEvent {
	fn context(&self) -> &String;
	fn settings(&self) -> &SettingsValue;
}

impl ActionEvent for PluginEvent {
	fn context(&self) -> &String {
		&self.context
	}

	fn settings(&self) -> &SettingsValue {
		&self.payload.settings
	}
}

#[derive(Default)]
pub struct OutboundEventManager;

impl OutboundEventManager {
	pub async fn set_title(
		&mut self,
		context: String,
		title: Option<String>,
		state: Option<u16>,
	) -> EventHandlerResult {
		if let Some(instance) = openaction::get_instance(context).await {
			instance.set_title(title, state).await?;
		}
		Ok(())
	}

	pub async fn set_image(
		&mut self,
		context: String,
		image: Option<String>,
		state: Option<u16>,
	) -> EventHandlerResult {
		if let Some(instance) = openaction::get_instance(context).await {
			instance.set_image(image, state).await?;
		}
		Ok(())
	}

	pub async fn show_alert(&mut self, context: String) -> EventHandlerResult {
		if let Some(instance) = openaction::get_instance(context).await {
			instance.show_alert().await?;
		}
		Ok(())
	}

	pub async fn send_event(&mut self, event: impl Serialize) -> EventHandlerResult {
		openaction::send_arbitrary_json(event).await?;
		Ok(())
	}
}

pub static OUTBOUND_EVENT_MANAGER: LazyLock<Mutex<Option<OutboundEventManager>>> =
	LazyLock::new(|| Mutex::new(Some(OutboundEventManager)));
