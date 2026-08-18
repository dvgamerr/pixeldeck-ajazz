mod audio;
mod compat;
mod device_brightness;
mod input_simulation;
mod live;
mod pixel;
mod profile_pagination;
mod run_command;
mod switch_profile;
mod system_monitor;

pub use compat::ActionEvent;
use compat::{
	DialPressEvent, DialRotateEvent, EventHandlerResult, EventPayload, KeyEvent,
	OutboundEventManager, PluginEvent, SettingsValue,
};
use openaction::{
	Action, ActionUuid, Instance, OpenActionResult, async_trait, register_action, run,
};

fn event(instance: &Instance, settings: &SettingsValue, ticks: i16) -> PluginEvent {
	PluginEvent {
		action: instance.action_uuid.clone(),
		context: instance.instance_id.clone(),
		device: instance.device_id.clone(),
		payload: EventPayload {
			settings: settings.clone(),
			ticks,
		},
	}
}

fn report(action: &str, result: EventHandlerResult) -> OpenActionResult<()> {
	if let Err(error) = result {
		log::warn!("{action} action failed: {error:#}");
	}
	Ok(())
}

fn key_down(event: KeyEvent) -> EventHandlerResult {
	match event.action.as_str() {
		"com.amansprojects.starterpack.runcommand" => run_command::down_up("down", event),
		"com.amansprojects.starterpack.inputsimulation" => {
			tokio::spawn(async move {
				if let Err(error) = input_simulation::down_up("down", event).await {
					log::warn!("Input simulation failed: {error:#}");
				}
			});
			Ok(())
		}
		_ => Ok(()),
	}
}

async fn key_up(event: KeyEvent, outbound: &mut OutboundEventManager) -> EventHandlerResult {
	match event.action.as_str() {
		"com.amansprojects.starterpack.runcommand" => run_command::down_up("up", event),
		"com.amansprojects.starterpack.inputsimulation" => {
			input_simulation::down_up("up", event).await
		}
		"com.amansprojects.starterpack.switchprofile" => {
			switch_profile::key_up(event, outbound).await
		}
		"com.amansprojects.starterpack.devicebrightness" => {
			device_brightness::up(event, outbound).await
		}
		_ => Ok(()),
	}
}

fn dial_down(event: DialPressEvent) -> EventHandlerResult {
	match event.action.as_str() {
		"com.amansprojects.starterpack.runcommand" => run_command::down_up("down", event),
		"com.amansprojects.starterpack.inputsimulation" => {
			tokio::spawn(async move {
				if let Err(error) = input_simulation::down_up("down", event).await {
					log::warn!("Input simulation failed: {error:#}");
				}
			});
			Ok(())
		}
		_ => Ok(()),
	}
}

async fn dial_up(event: DialPressEvent, outbound: &mut OutboundEventManager) -> EventHandlerResult {
	match event.action.as_str() {
		"com.amansprojects.starterpack.runcommand" => run_command::down_up("up", event),
		"com.amansprojects.starterpack.inputsimulation" => {
			input_simulation::down_up("up", event).await
		}
		"com.amansprojects.starterpack.devicebrightness" => {
			device_brightness::up(event, outbound).await
		}
		"com.amansprojects.starterpack.systemvolume" => audio::press_device(event, outbound).await,
		_ => Ok(()),
	}
}

async fn dial_rotate(
	event: DialRotateEvent,
	outbound: &mut OutboundEventManager,
) -> EventHandlerResult {
	match event.action.as_str() {
		profile_pagination::ACTION => profile_pagination::rotate(event, outbound).await,
		"com.amansprojects.starterpack.runcommand" => run_command::rotate(event),
		"com.amansprojects.starterpack.inputsimulation" => input_simulation::rotate(event).await,
		"com.amansprojects.starterpack.devicebrightness" => {
			device_brightness::rotate(event, outbound).await
		}
		"com.amansprojects.starterpack.systemvolume" => audio::rotate_volume(event, outbound).await,
		_ => Ok(()),
	}
}

async fn will_appear(
	event: PluginEvent,
	outbound: &mut OutboundEventManager,
) -> EventHandlerResult {
	match event.action.as_str() {
		audio::ACTION => audio::appear(event.context, outbound).await,
		profile_pagination::ACTION => {
			profile_pagination::appear(event.context, event.payload.settings, outbound).await
		}
		system_monitor::ACTION => {
			system_monitor::appear(event.context, event.payload.settings, outbound).await
		}
		_ => Ok(()),
	}
}

fn will_disappear(event: PluginEvent) {
	match event.action.as_str() {
		audio::ACTION => audio::disappear(&event.context),
		system_monitor::ACTION => system_monitor::disappear(&event.context),
		_ => {}
	}
}

async fn did_receive_settings(
	event: PluginEvent,
	outbound: &mut OutboundEventManager,
) -> EventHandlerResult {
	match event.action.as_str() {
		audio::ACTION => audio::refresh(event.context, outbound).await,
		profile_pagination::ACTION => {
			profile_pagination::refresh(event.context, event.payload.settings, outbound).await
		}
		system_monitor::ACTION => {
			system_monitor::refresh(event.context, event.payload.settings, outbound).await
		}
		_ => Ok(()),
	}
}

macro_rules! starter_action {
	($name:ident, $uuid:literal) => {
		struct $name;

		#[async_trait]
		impl Action for $name {
			const UUID: ActionUuid = $uuid;
			type Settings = SettingsValue;

			async fn key_down(
				&self,
				instance: &Instance,
				settings: &SettingsValue,
			) -> OpenActionResult<()> {
				report(Self::UUID, key_down(event(instance, settings, 0)))
			}

			async fn key_up(
				&self,
				instance: &Instance,
				settings: &SettingsValue,
			) -> OpenActionResult<()> {
				let mut outbound = OutboundEventManager;
				report(
					Self::UUID,
					key_up(event(instance, settings, 0), &mut outbound).await,
				)
			}

			async fn dial_down(
				&self,
				instance: &Instance,
				settings: &SettingsValue,
			) -> OpenActionResult<()> {
				report(Self::UUID, dial_down(event(instance, settings, 0)))
			}

			async fn dial_up(
				&self,
				instance: &Instance,
				settings: &SettingsValue,
			) -> OpenActionResult<()> {
				let mut outbound = OutboundEventManager;
				report(
					Self::UUID,
					dial_up(event(instance, settings, 0), &mut outbound).await,
				)
			}

			async fn dial_rotate(
				&self,
				instance: &Instance,
				settings: &SettingsValue,
				ticks: i16,
				_pressed: bool,
			) -> OpenActionResult<()> {
				let mut outbound = OutboundEventManager;
				report(
					Self::UUID,
					dial_rotate(event(instance, settings, ticks), &mut outbound).await,
				)
			}

			async fn will_appear(
				&self,
				instance: &Instance,
				settings: &SettingsValue,
			) -> OpenActionResult<()> {
				let mut outbound = OutboundEventManager;
				report(
					Self::UUID,
					will_appear(event(instance, settings, 0), &mut outbound).await,
				)
			}

			async fn will_disappear(
				&self,
				instance: &Instance,
				settings: &SettingsValue,
			) -> OpenActionResult<()> {
				will_disappear(event(instance, settings, 0));
				Ok(())
			}

			async fn did_receive_settings(
				&self,
				instance: &Instance,
				settings: &SettingsValue,
			) -> OpenActionResult<()> {
				let mut outbound = OutboundEventManager;
				report(
					Self::UUID,
					did_receive_settings(event(instance, settings, 0), &mut outbound).await,
				)
			}
		}
	};
}

starter_action!(RunCommandAction, "com.amansprojects.starterpack.runcommand");
starter_action!(
	InputSimulationAction,
	"com.amansprojects.starterpack.inputsimulation"
);
starter_action!(
	SwitchProfileAction,
	"com.amansprojects.starterpack.switchprofile"
);
starter_action!(
	ProfilePaginationAction,
	"com.amansprojects.starterpack.profilepagination"
);
starter_action!(
	DeviceBrightnessAction,
	"com.amansprojects.starterpack.devicebrightness"
);
starter_action!(
	SystemMonitorAction,
	"com.amansprojects.starterpack.systemmonitor"
);
starter_action!(
	SystemVolumeAction,
	"com.amansprojects.starterpack.systemvolume"
);

#[tokio::main]
async fn main() -> OpenActionResult<()> {
	simplelog::TermLogger::init(
		simplelog::LevelFilter::Debug,
		simplelog::Config::default(),
		simplelog::TerminalMode::Stdout,
		simplelog::ColorChoice::Never,
	)
	.expect("failed to initialise logging");

	register_action(RunCommandAction).await;
	register_action(InputSimulationAction).await;
	register_action(SwitchProfileAction).await;
	register_action(ProfilePaginationAction).await;
	register_action(DeviceBrightnessAction).await;
	register_action(SystemMonitorAction).await;
	register_action(SystemVolumeAction).await;
	run(std::env::args().collect()).await
}
