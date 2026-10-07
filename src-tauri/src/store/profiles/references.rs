use crate::shared::{ActionInstance, Profile};
use crate::store::Store;

pub(super) fn profile_belongs_to_device(profile: &Profile, device: &str) -> bool {
	profile.keys.iter().chain(&profile.sliders).flatten().next().is_some_and(|instance| instance.context.device == device)
}

/// Points a "switch profile" action at `new_id`, or clears it when `new_id` is `None`.
fn update_switch_profile_setting(settings: &mut serde_json::Map<String, serde_json::Value>, device: &str, old_id: &str, new_id: Option<&str>) -> bool {
	let targets_device = settings.get("device").and_then(|value| value.as_str()).is_none_or(|target| target == device);
	if !targets_device || settings.get("profile").and_then(|value| value.as_str()) != Some(old_id) {
		return false;
	}
	match new_id {
		Some(new_id) => settings.insert("profile".to_owned(), serde_json::Value::String(new_id.to_owned())),
		None => settings.remove("profile"),
	};
	true
}

/// Renames or drops `old_id` within a "profile pagination" action's profile list.
fn update_pagination_setting(settings: &mut serde_json::Map<String, serde_json::Value>, old_id: &str, new_id: Option<&str>) -> bool {
	let Some(profiles) = settings.get_mut("profiles").and_then(serde_json::Value::as_array_mut) else {
		return false;
	};
	let mut changed = false;
	let mut updated = Vec::with_capacity(profiles.len());
	for mut profile in profiles.drain(..) {
		if profile.as_str() == Some(old_id) {
			changed = true;
			let Some(new_id) = new_id else { continue };
			profile = serde_json::Value::String(new_id.to_owned());
		}
		if !updated.contains(&profile) {
			updated.push(profile);
		}
	}
	*profiles = updated;
	changed
}

pub(super) fn update_instance_references(instance: &mut ActionInstance, device: &str, old_id: &str, new_id: Option<&str>) -> bool {
	let mut changed = false;
	if let Some(settings) = instance.settings.as_object_mut() {
		if instance.action.uuid == "com.amansprojects.starterpack.switchprofile" {
			changed |= update_switch_profile_setting(settings, device, old_id, new_id);
		}
		if instance.action.uuid == "com.amansprojects.starterpack.profilepagination" {
			changed |= update_pagination_setting(settings, old_id, new_id);
		}
	}
	for child in instance.children.iter_mut().flatten() {
		changed |= update_instance_references(child, device, old_id, new_id);
	}
	changed
}

pub(super) fn rename_profile_contents(profile: &mut Profile, new_id: &str, old_images: &std::path::Path, new_images: &std::path::Path) {
	fn rename_instance(instance: &mut ActionInstance, new_id: &str, old_images: &std::path::Path, new_images: &std::path::Path) {
		instance.context.profile = new_id.to_owned();
		for state in &mut instance.states {
			let path = std::path::Path::new(&state.image);
			if path.starts_with(old_images) {
				state.image = new_images.join(path.strip_prefix(old_images).unwrap()).to_string_lossy().into_owned();
			}
		}
		if let Some(children) = &mut instance.children {
			for child in children {
				rename_instance(child, new_id, old_images, new_images);
			}
		}
	}

	profile.id = new_id.to_owned();
	for instance in profile.keys.iter_mut().chain(&mut profile.sliders).flatten() {
		rename_instance(instance, new_id, old_images, new_images);
	}
}

/// Rewrites switch-profile and pagination references inside one profile store, saving it when changed.
pub(super) fn update_store_references(store: &mut Store<Profile>, device: &str, old_id: &str, new_id: Option<&str>) -> Result<(), anyhow::Error> {
	if !profile_belongs_to_device(&store.value, device) {
		return Ok(());
	}
	let mut changed = false;
	for instance in store.value.keys.iter_mut().chain(&mut store.value.sliders).flatten() {
		changed |= update_instance_references(instance, device, old_id, new_id);
	}
	if changed {
		store.save()?;
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::rename_profile_contents;
	use crate::shared::{Action, ActionContext, ActionInstance, ActionState, Profile};
	use std::path::PathBuf;

	#[test]
	fn rename_profile_updates_nested_contexts_and_image_paths() {
		fn instance(profile: &str, image: String, children: Option<Vec<ActionInstance>>) -> ActionInstance {
			ActionInstance {
				action: Action {
					name: "Test".to_owned(),
					uuid: "test.action".to_owned(),
					plugin: "test".to_owned(),
					tooltip: String::new(),
					icon: String::new(),
					disable_automatic_states: false,
					visible_in_action_list: true,
					supported_in_multi_actions: true,
					property_inspector: String::new(),
					controllers: vec!["Keypad".to_owned()],
					states: vec![ActionState::default()],
				},
				context: ActionContext {
					device: "device".to_owned(),
					profile: profile.to_owned(),
					controller: "Keypad".to_owned(),
					position: 0,
					index: 0,
				},
				states: vec![ActionState { image, ..ActionState::default() }],
				current_state: 0,
				settings: serde_json::Value::Null,
				children,
			}
		}

		let old_images = PathBuf::from("images").join("device").join("Old");
		let new_images = PathBuf::from("images").join("device").join("New");
		let child = instance("Old", old_images.join("child.png").to_string_lossy().into_owned(), None);
		let parent = instance("Old", old_images.join("parent.png").to_string_lossy().into_owned(), Some(vec![child]));
		let mut profile = Profile {
			id: "Old".to_owned(),
			keys: vec![Some(parent)],
			sliders: vec![],
		};

		rename_profile_contents(&mut profile, "New", &old_images, &new_images);

		let parent = profile.keys[0].as_ref().unwrap();
		let child = &parent.children.as_ref().unwrap()[0];
		assert_eq!(profile.id, "New");
		assert_eq!(parent.context.profile, "New");
		assert_eq!(child.context.profile, "New");
		assert_eq!(parent.states[0].image, new_images.join("parent.png").to_string_lossy());
		assert_eq!(child.states[0].image, new_images.join("child.png").to_string_lossy());
	}
}
