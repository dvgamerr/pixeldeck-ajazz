use super::Error;

use crate::shared::{Action, ActionContext, ActionInstance, Context, config_dir};
use crate::store::profiles::{LocksMut, acquire_locks_mut, get_instance_mut, get_slot_mut, save_profile};

use tauri::{AppHandle, Emitter, Manager, command};
use tokio::fs::remove_dir_all;

#[command]
pub async fn create_instance(app: AppHandle, action: Action, context: Context) -> Result<Option<ActionInstance>, Error> {
	if !action.controllers.contains(&context.controller) {
		return Ok(None);
	}

	let mut locks = acquire_locks_mut().await;
	let slot = get_slot_mut(&context, &mut locks).await?;

	if let Some(parent) = slot {
		let Some(children) = &mut parent.children else { return Ok(None) };
		let index = match children.last() {
			None => 1,
			Some(instance) => instance.context.index + 1,
		};

		let instance = ActionInstance {
			action: action.clone(),
			context: ActionContext::from_context(context.clone(), index),
			states: action.states.clone(),
			current_state: 0,
			settings: serde_json::Value::Object(serde_json::Map::new()),
			children: None,
		};
		children.push(instance.clone());

		if parent.action.uuid == "opendeck.toggleaction" && parent.states.len() < children.len() {
			parent.states.push(crate::shared::ActionState {
				image: "opendeck/toggle-action.png".to_owned(),
				..Default::default()
			});
			let _ = update_state(&app, parent.context.clone(), &mut locks).await;
		}

		save_profile(&context.device, &mut locks).await?;
		let _ = crate::events::outbound::will_appear::will_appear(&instance).await;

		Ok(Some(instance))
	} else {
		let instance = ActionInstance {
			action: action.clone(),
			context: ActionContext::from_context(context.clone(), 0),
			states: action.states.clone(),
			current_state: 0,
			settings: serde_json::Value::Object(serde_json::Map::new()),
			children: if matches!(action.uuid.as_str(), "opendeck.multiaction" | "opendeck.toggleaction") {
				Some(vec![])
			} else {
				None
			},
		};

		*slot = Some(instance.clone());
		let slot = slot.clone();

		save_profile(&context.device, &mut locks).await?;
		let _ = crate::events::outbound::will_appear::will_appear(&instance).await;

		Ok(slot)
	}
}

fn instance_images_dir(context: &ActionContext) -> std::path::PathBuf {
	config_dir()
		.join("images")
		.join(&context.device)
		.join(&context.profile)
		.join(format!("{}.{}.{}", context.controller, context.position, context.index))
}

/// Re-parents the children of a moved container and resets their state images to the action defaults.
fn rebase_children(instance: &mut ActionInstance, destination: &Context) {
	let Some(children) = &mut instance.children else {
		return;
	};
	for (index, child) in children.iter_mut().enumerate() {
		child.context = ActionContext::from_context(destination.clone(), index as u16 + 1);
		for (i, state) in child.states.iter_mut().enumerate() {
			let default_image = &child.action.states[i].image;
			state.image = if default_image.is_empty() { child.action.icon.clone() } else { default_image.clone() };
		}
	}
}

async fn copy_instance_images(old_dir: &std::path::Path, new_dir: &std::path::Path) {
	let _ = tokio::fs::create_dir_all(new_dir).await;
	let Ok(files) = old_dir.read_dir() else {
		return;
	};
	for file in files.flatten() {
		let _ = tokio::fs::copy(file.path(), new_dir.join(file.file_name())).await;
	}
}

fn relocate_state_images(instance: &mut ActionInstance, old_dir: &std::path::Path, new_dir: &std::path::Path) {
	for state in instance.states.iter_mut() {
		let path = std::path::Path::new(&state.image);
		if let Ok(relative) = path.strip_prefix(old_dir) {
			state.image = new_dir.join(relative).to_string_lossy().into_owned();
		}
	}
}

#[command]
pub async fn move_instance(source: Context, destination: Context, retain: bool) -> Result<Option<ActionInstance>, Error> {
	if source.controller != destination.controller {
		return Ok(None);
	}

	let mut locks = acquire_locks_mut().await;
	let src = get_slot_mut(&source, &mut locks).await?;

	let Some(mut new) = src.clone() else {
		return Ok(None);
	};
	let old_dir = instance_images_dir(&new.context);
	new.context = ActionContext::from_context(destination.clone(), 0);
	rebase_children(&mut new, &destination);

	let new_dir = instance_images_dir(&new.context);
	copy_instance_images(&old_dir, &new_dir).await;
	relocate_state_images(&mut new, &old_dir, &new_dir);

	let dst = get_slot_mut(&destination, &mut locks).await?;
	if dst.is_some() {
		return Ok(None);
	}
	*dst = Some(new.clone());

	if !retain {
		let src = get_slot_mut(&source, &mut locks).await?;
		if let Some(old) = src {
			let _ = crate::events::outbound::will_appear::will_disappear(old, true).await;
			let _ = remove_dir_all(instance_images_dir(&old.context)).await;
		}
		*src = None;
	}

	let _ = crate::events::outbound::will_appear::will_appear(&new).await;

	save_profile(&destination.device, &mut locks).await?;

	Ok(Some(new))
}

async fn disappear_and_remove_images(instance: &ActionInstance) {
	let _ = crate::events::outbound::will_appear::will_disappear(instance, true).await;
	let _ = remove_dir_all(instance_images_dir(&instance.context)).await;
}

/// Removes a top-level instance together with its children.
async fn remove_container(instance: &ActionInstance) {
	let _ = crate::events::outbound::will_appear::will_disappear(instance, true).await;
	for child in instance.children.iter().flatten() {
		disappear_and_remove_images(child).await;
	}
	let _ = remove_dir_all(instance_images_dir(&instance.context)).await;
}

/// Removes a child of a container instance. Returns the container's context when
/// its toggle states changed and the frontend needs to be refreshed.
async fn remove_child(container: &mut ActionInstance, context: &ActionContext) -> Option<ActionContext> {
	let children = container.children.as_mut().unwrap();
	if let Some(index) = children.iter().position(|child| child.context == *context) {
		disappear_and_remove_images(&children[index]).await;
		children.remove(index);
	}

	if container.action.uuid != "opendeck.toggleaction" {
		return None;
	}
	if container.current_state as usize >= children.len() {
		container.current_state = if children.is_empty() { 0 } else { children.len() as u16 - 1 };
	}
	if children.is_empty() {
		return None;
	}
	container.states.pop();
	Some(container.context.clone())
}

#[command]
pub async fn remove_instance(context: ActionContext) -> Result<(), Error> {
	let mut locks = acquire_locks_mut().await;
	let slot = get_slot_mut(&(&context).into(), &mut locks).await?;
	let Some(instance) = slot else {
		return Ok(());
	};

	if instance.context == context {
		remove_container(instance).await;
		*slot = None;
	} else if let Some(container_context) = remove_child(instance, &context).await {
		let _ = update_state(crate::APP_HANDLE.get().unwrap(), container_context, &mut locks).await;
	}

	save_profile(&context.device, &mut locks).await?;

	Ok(())
}

#[derive(Clone, serde::Serialize)]
struct UpdateStateEvent {
	context: ActionContext,
	contents: Option<ActionInstance>,
}

pub async fn update_state(app: &AppHandle, context: ActionContext, locks: &mut LocksMut<'_>) -> Result<(), anyhow::Error> {
	let window = app.get_webview_window("main").unwrap();
	window.emit(
		"update_state",
		UpdateStateEvent {
			contents: get_instance_mut(&context, locks).await?.cloned(),
			context,
		},
	)?;
	Ok(())
}

#[command]
pub async fn set_state(instance: ActionInstance, state: u16) -> Result<(), Error> {
	let mut locks = acquire_locks_mut().await;
	let reference = get_instance_mut(&instance.context, &mut locks).await?.unwrap();
	*reference = instance.clone();
	save_profile(&instance.context.device, &mut locks).await?;
	crate::events::outbound::states::title_parameters_did_change(&instance, state).await?;
	Ok(())
}

#[command]
pub async fn update_image(context: Context, image: String) -> Result<(), Error> {
	update_images(vec![DeviceFrameUpdate { context, image: Some(image) }]).await
}

#[derive(serde::Deserialize)]
pub struct DeviceFrameUpdate {
	context: Context,
	image: Option<String>,
}

#[command]
pub async fn update_images(frames: Vec<DeviceFrameUpdate>) -> Result<(), Error> {
	let Some(first) = frames.first() else {
		return Ok(());
	};
	if frames.len() > 64 {
		return Err(Error::new("An image batch cannot contain more than 64 frames".to_owned()));
	}
	if frames
		.iter()
		.any(|frame| frame.context.device != first.context.device || frame.context.profile != first.context.profile)
	{
		return Err(Error::new("Every frame in an image batch must target the same device and profile".to_owned()));
	}

	// Keep the selected-profile lock until the batch has reached the device. A
	// competing profile switch therefore cannot let an older batch commit afterward.
	let mut device_stores = crate::store::profiles::DEVICE_STORES.write().await;
	if device_stores.get_selected_profile(&first.context.device).ok().as_ref() != Some(&first.context.profile) {
		return Ok(());
	}

	let updates = frames.into_iter().map(|frame| (frame.context, frame.image)).collect();
	crate::events::outbound::devices::update_images(updates).await?;
	Ok(())
}

#[derive(Clone, serde::Serialize)]
struct KeyMovedEvent {
	context: Context,
	pressed: bool,
}

pub async fn key_moved(app: &AppHandle, context: Context, pressed: bool) -> Result<(), anyhow::Error> {
	let window = app.get_webview_window("main").unwrap();
	window.emit("key_moved", KeyMovedEvent { context, pressed })?;
	Ok(())
}
