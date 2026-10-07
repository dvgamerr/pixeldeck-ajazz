use super::Error;

use crate::shared::DEVICES;
use crate::store::profiles::{acquire_locks_mut, get_device_profiles};

use tauri::{AppHandle, Emitter, Manager, command};

fn connected_device(device: &str) -> Result<crate::shared::DeviceInfo, Error> {
	DEVICES.get(device).map(|device| device.clone()).ok_or_else(|| Error::new(format!("device {device} not found")))
}

fn is_valid_profile_segment(segment: &str) -> bool {
	!segment.is_empty() && segment.chars().all(|character| character.is_ascii_alphanumeric() || character == '_' || character == ' ')
}

fn validate_profile_id(id: &str) -> Result<(), anyhow::Error> {
	if id.is_empty() || id.trim() != id {
		return Err(anyhow::anyhow!("Profile name cannot be empty or start or end with spaces"));
	}
	let segments = id.split('/').collect::<Vec<_>>();
	if segments.len() > 2 || !segments.iter().all(|segment| is_valid_profile_segment(segment)) {
		return Err(anyhow::anyhow!("Profile names may contain letters, numbers, spaces, underscores, and one folder separator"));
	}
	Ok(())
}

fn visible_profile_instances(profile: &crate::shared::Profile) -> impl Iterator<Item = &crate::shared::ActionInstance> {
	profile.keys.iter().flatten().chain(profile.sliders.iter().flatten()).flat_map(|instance| {
		let is_container = matches!(instance.action.uuid.as_str(), "opendeck.multiaction" | "opendeck.toggleaction");
		let parent = (!is_container).then_some(instance).into_iter();
		let children = instance.children.as_deref().filter(|_| is_container).into_iter().flatten();
		parent.chain(children)
	})
}

async fn profile_will_appear(profile: &crate::shared::Profile) {
	for instance in visible_profile_instances(profile) {
		let _ = crate::events::outbound::will_appear::will_appear(instance).await;
	}
}

async fn profile_will_disappear(profile: &crate::shared::Profile) {
	for instance in visible_profile_instances(profile) {
		let _ = crate::events::outbound::will_appear::will_disappear(instance, false).await;
	}
}

#[command]
pub fn get_profiles(device: &str) -> Result<Vec<String>, Error> {
	Ok(get_device_profiles(device)?)
}

#[command]
pub async fn get_selected_profile(device: String) -> Result<crate::shared::Profile, Error> {
	let device_info = connected_device(&device)?;
	let mut locks = acquire_locks_mut().await;
	let selected_profile = locks.device_stores.get_selected_profile(&device)?;
	let profile = locks.profile_stores.get_profile_store(&device_info, &selected_profile)?;

	Ok(profile.value.clone())
}

#[allow(clippy::flat_map_identity)]
#[command]
pub async fn reload_selected_profile(device: String) -> Result<crate::shared::Profile, Error> {
	let device_info = connected_device(&device)?;
	let mut locks = acquire_locks_mut().await;
	crate::ajazz::resume_profile_rendering(&device).await;
	let selected_profile = locks.device_stores.get_selected_profile(&device)?;
	crate::events::outbound::devices::clear_screen(device.clone()).await?;

	let profile = locks.profile_stores.get_profile_store(&device_info, &selected_profile)?;
	profile_will_appear(&profile.value).await;

	Ok(profile.value.clone())
}

#[allow(clippy::flat_map_identity)]
#[command]
pub async fn set_selected_profile(device: String, id: String) -> Result<(), Error> {
	let device_info = connected_device(&device)?;
	let mut locks = acquire_locks_mut().await;
	let selected_profile = locks.device_stores.get_selected_profile(&device)?;
	if selected_profile == id {
		return Ok(());
	}
	let old_profile = locks.profile_stores.get_profile_store(&device_info, &selected_profile)?.value.clone();
	profile_will_disappear(&old_profile).await;

	// We must use the mutable version of get_profile_store in order to create the store if it does not exist.
	let store = match locks.profile_stores.get_profile_store_mut(&device_info, &id).await {
		Ok(store) => store,
		Err(error) => {
			profile_will_appear(&old_profile).await;
			return Err(error.into());
		}
	};
	let new_profile = &store.value;
	profile_will_appear(new_profile).await;
	store.save()?;

	locks.device_stores.set_selected_profile(&device, id)?;

	Ok(())
}

#[command]
pub async fn rename_profile(device: String, profile: String, new_id: String) -> Result<crate::shared::Profile, Error> {
	validate_profile_id(&new_id)?;
	let mut locks = acquire_locks_mut().await;
	let device_info = connected_device(&device)?;
	let profiles = get_device_profiles(&device)?;
	if !profiles.contains(&profile) {
		return Err(Error::new(format!("profile {profile} not found")));
	}
	if profile == new_id {
		let selected = locks.device_stores.get_selected_profile(&device)?;
		return Ok(locks.profile_stores.get_profile_store(&device_info, &selected)?.value.clone());
	}
	if profiles.contains(&new_id) {
		return Err(Error::new(format!("profile {new_id} already exists")));
	}

	let selected_before = locks.device_stores.get_selected_profile(&device)?;
	let old_profile = locks.profile_stores.get_profile_store(&device_info, &profile)?.value.clone();
	if selected_before == profile {
		profile_will_disappear(&old_profile).await;
	}

	let renamed_profile = match locks.profile_stores.rename_profile(&device_info, &profile, &new_id) {
		Ok(profile) => profile,
		Err(error) => {
			if selected_before == profile {
				profile_will_appear(&old_profile).await;
			}
			return Err(error.into());
		}
	};
	locks.profile_stores.update_profile_action_references(&device, &profile, Some(&new_id))?;
	locks.device_stores.rename_profile_references(&device, &profile, &new_id)?;
	let selected_after = locks.device_stores.get_selected_profile(&device)?;
	let selected_profile = if selected_after == new_id {
		profile_will_appear(&renamed_profile).await;
		renamed_profile
	} else {
		locks.profile_stores.get_profile_store(&device_info, &selected_after)?.value.clone()
	};
	drop(locks);

	update_application_profile_references(&device, &profile, Some(&new_id)).await?;
	Ok(selected_profile)
}

#[command]
pub async fn delete_profile(device: String, profile: String) -> Result<(), Error> {
	let mut locks = acquire_locks_mut().await;
	let selected_profile = locks.device_stores.get_selected_profile(&device)?;
	if selected_profile == profile {
		return Err(Error::new("The selected profile cannot be deleted".to_owned()));
	}
	locks.profile_stores.delete_profile(&device, &profile);
	locks.profile_stores.update_profile_action_references(&device, &profile, None)?;
	drop(locks);
	update_application_profile_references(&device, &profile, None).await?;
	Ok(())
}

async fn update_application_profile_references(device: &str, old_id: &str, new_id: Option<&str>) -> Result<(), Error> {
	let mut store = crate::application_watcher::APPLICATION_PROFILES.write().await;
	for devices in store.value.values_mut() {
		if devices.get(device).map(String::as_str) == Some(old_id) {
			if let Some(new_id) = new_id {
				devices.insert(device.to_owned(), new_id.to_owned());
			} else {
				devices.remove(device);
			}
		}
	}
	store.value.retain(|_, devices| !devices.is_empty());
	store.save()?;
	drop(store);

	let mut previous_profiles = crate::application_watcher::PREVIOUS_PROFILES.write().await;
	if previous_profiles.get(device).map(String::as_str) == Some(old_id) {
		if let Some(new_id) = new_id {
			previous_profiles.insert(device.to_owned(), new_id.to_owned());
		} else {
			previous_profiles.remove(device);
		}
	}
	Ok(())
}

pub async fn rerender_images(app: &AppHandle) -> Result<(), anyhow::Error> {
	let window = app.get_webview_window("main").unwrap();
	window.emit("rerender_images", ())?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::validate_profile_id;

	#[test]
	fn profile_names_accept_one_optional_folder() {
		assert!(validate_profile_id("Default").is_ok());
		assert!(validate_profile_id("Work/Editing 2").is_ok());
		assert!(validate_profile_id(" Work").is_err());
		assert!(validate_profile_id("folder/nested/profile").is_err());
		assert!(validate_profile_id("folder/profile.name").is_err());
	}
}
