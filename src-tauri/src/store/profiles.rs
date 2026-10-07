use super::Store;

use crate::shared::{ActionInstance, DEVICES, DeviceInfo, Profile, config_dir};

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, anyhow, bail};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

pub struct ProfileStores {
	stores: HashMap<String, Store<Profile>>,
}

impl ProfileStores {
	fn canonical_id(device: &str, id: &str) -> String {
		if cfg!(target_os = "windows") {
			PathBuf::from(device).join(id.replace('/', "\\")).to_str().unwrap().to_owned()
		} else {
			PathBuf::from(device).join(id).to_str().unwrap().to_owned()
		}
	}

	pub fn get_profile_store(&self, device: &DeviceInfo, id: &str) -> Result<&Store<Profile>, anyhow::Error> {
		self.stores.get(&Self::canonical_id(&device.id, id)).ok_or_else(|| anyhow!("profile not found"))
	}

	pub async fn get_profile_store_mut(&mut self, device: &DeviceInfo, id: &str) -> Result<&mut Store<Profile>, anyhow::Error> {
		let canonical_id = Self::canonical_id(&device.id, id);
		if self.stores.contains_key(&canonical_id) {
			Ok(self.stores.get_mut(&canonical_id).unwrap())
		} else {
			let default = Profile {
				id: id.to_owned(),
				keys: Vec::new(),
				sliders: Vec::new(),
			};

			let mut store = Store::new(&canonical_id, &config_dir().join("profiles"), default).context(format!("Failed to create store for profile {}", canonical_id))?;
			store.value.keys.resize((device.rows * device.columns) as usize, None);
			store.value.sliders.resize(device.encoders as usize, None);

			let categories = crate::shared::CATEGORIES.read().await;
			let actions = categories.values().flat_map(|v| v.actions.iter()).collect::<Vec<_>>();
			let plugins_dir = config_dir().join("plugins");
			let registered = crate::events::registered_plugins().await;
			let keep_instance = |instance: &ActionInstance| -> bool {
				instance.action.plugin == "opendeck"
					|| (plugins_dir.join(&instance.action.plugin).exists() && (!registered.contains(&instance.action.plugin) || actions.iter().any(|v| v.uuid == instance.action.uuid)))
			};
			for slot in store.value.keys.iter_mut().chain(store.value.sliders.iter_mut()) {
				if let Some(instance) = slot {
					if !keep_instance(instance) {
						*slot = None;
					} else if let Some(children) = &mut instance.children {
						children.retain_mut(|child| keep_instance(child));
					}
				}
			}
			store.save()?;

			self.stores.insert(canonical_id.clone(), store);
			Ok(self.stores.get_mut(&canonical_id).unwrap())
		}
	}

	pub fn remove_profile(&mut self, device: &str, id: &str) {
		self.stores.remove(&Self::canonical_id(device, id));
	}

	pub fn delete_profile(&mut self, device: &str, id: &str) {
		self.remove_profile(device, id);
		let config_dir = config_dir();
		#[cfg(target_os = "windows")]
		let id = &id.replace('/', "\\");
		let path = config_dir.join("profiles").join(device).join(format!("{id}.json"));
		let _ = fs::remove_file(&path);
		// This is safe as `remove_dir` errors if the directory is not empty.
		let _ = fs::remove_dir(path.parent().unwrap());
		let images_path = config_dir.join("images").join(device).join(id);
		let _ = fs::remove_dir_all(images_path);
	}

	pub fn rename_profile(&mut self, device: &DeviceInfo, old_id: &str, new_id: &str) -> Result<Profile, anyhow::Error> {
		let old_key = Self::canonical_id(&device.id, old_id);
		let new_key = Self::canonical_id(&device.id, new_id);
		let old_store = self.stores.get(&old_key).ok_or_else(|| anyhow!("profile not found"))?;
		let mut renamed_profile = old_store.value.clone();

		let profiles_dir = config_dir().join("profiles");
		let old_path = profiles_dir.join(format!("{old_key}.json"));
		let new_path = profiles_dir.join(format!("{new_key}.json"));
		if self.stores.contains_key(&new_key) || new_path.exists() {
			bail!("profile {new_id} already exists");
		}

		let old_images = config_dir().join("images").join(&device.id).join(PathBuf::from(old_id));
		let new_images = config_dir().join("images").join(&device.id).join(PathBuf::from(new_id));
		if old_images.exists() && new_images.exists() {
			bail!("profile image directory for {new_id} already exists");
		}

		if old_images.exists() {
			fs::create_dir_all(new_images.parent().unwrap())?;
			fs::rename(&old_images, &new_images)?;
		}

		rename_profile_contents(&mut renamed_profile, new_id, &old_images, &new_images);
		let mut new_store = Store::new(&new_key, &profiles_dir, renamed_profile.clone())?;
		new_store.value = renamed_profile.clone();
		if let Err(error) = new_store.save() {
			let _ = fs::remove_file(&new_path);
			if new_images.exists() {
				let _ = fs::rename(&new_images, &old_images);
			}
			return Err(error);
		}

		if let Err(error) = fs::remove_file(&old_path) {
			let _ = fs::remove_file(&new_path);
			if new_images.exists() {
				let _ = fs::rename(&new_images, &old_images);
			}
			return Err(error.into());
		}

		self.stores.remove(&old_key);
		self.stores.insert(new_key, new_store);
		if let Some(parent) = old_path.parent() {
			let _ = fs::remove_dir(parent);
		}
		if let Some(parent) = old_images.parent() {
			let _ = fs::remove_dir(parent);
		}

		Ok(renamed_profile)
	}

	pub fn update_profile_action_references(&mut self, device: &str, old_id: &str, new_id: Option<&str>) -> Result<(), anyhow::Error> {
		for store in self.stores.values_mut() {
			if !profile_belongs_to_device(&store.value, device) {
				continue;
			}
			let mut changed = false;
			for instance in store.value.keys.iter_mut().chain(&mut store.value.sliders).flatten() {
				changed |= update_instance_references(instance, device, old_id, new_id);
			}
			if changed {
				store.save()?;
			}
		}
		Ok(())
	}

	pub fn all_from_plugin(&self, plugin: &str) -> Vec<crate::shared::ActionContext> {
		let mut all = vec![];
		for store in self.stores.values() {
			for instance in store.value.keys.iter().chain(&store.value.sliders).flatten() {
				if instance.action.plugin == plugin {
					all.push(instance.context.clone());
				}
			}
		}
		all
	}
}

fn profile_belongs_to_device(profile: &Profile, device: &str) -> bool {
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

fn update_instance_references(instance: &mut ActionInstance, device: &str, old_id: &str, new_id: Option<&str>) -> bool {
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

fn rename_profile_contents(profile: &mut Profile, new_id: &str, old_images: &std::path::Path, new_images: &std::path::Path) {
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

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceConfig {
	pub selected_profile: String,
}

impl Default for DeviceConfig {
	fn default() -> Self {
		Self {
			selected_profile: "Default".to_owned(),
		}
	}
}

impl super::NotProfile for DeviceConfig {}

pub struct DeviceStores {
	stores: HashMap<String, Store<DeviceConfig>>,
}

impl DeviceStores {
	fn get_or_create(&mut self, device: &str) -> Result<&mut Store<DeviceConfig>, anyhow::Error> {
		if !self.stores.contains_key(device) {
			let store = Store::new(device, &config_dir().join("profiles"), DeviceConfig::default()).context(format!("Failed to create store for device config {device}"))?;
			store.save()?;
			self.stores.insert(device.to_owned(), store);
		}
		Ok(self.stores.get_mut(device).unwrap())
	}

	pub fn get_selected_profile(&mut self, device: &str) -> Result<String, anyhow::Error> {
		let from_store = self.get_or_create(device)?.value.selected_profile.clone();
		let all = get_device_profiles(device)?;
		if all.contains(&from_store) { Ok(from_store) } else { Ok(all.first().unwrap().clone()) }
	}

	pub fn set_selected_profile(&mut self, device: &str, id: String) -> Result<(), anyhow::Error> {
		let store = self.get_or_create(device)?;
		store.value.selected_profile = id;
		store.save()
	}

	pub fn rename_profile_references(&mut self, device: &str, old_id: &str, new_id: &str) -> Result<(), anyhow::Error> {
		let store = self.get_or_create(device)?;
		if store.value.selected_profile == old_id {
			store.value.selected_profile = new_id.to_owned();
		}
		store.save()
	}
}

fn profile_id_from_file_name(file_name: &OsStr) -> Option<String> {
	let mut id = file_name.to_string_lossy().into_owned();
	let suffix = [".json.temp", ".json.bak", ".json"].into_iter().find(|suffix| id.ends_with(suffix))?;
	id.truncate(id.len() - suffix.len());
	Some(id)
}

pub fn get_device_profiles(device: &str) -> Result<Vec<String>, anyhow::Error> {
	let mut profiles: Vec<String> = vec![];

	let device_path = config_dir().join("profiles").join(device);
	fs::create_dir_all(&device_path)?;
	let entries = fs::read_dir(device_path)?;

	for entry in entries.flatten() {
		let metadata = entry.metadata()?;
		if metadata.is_file() {
			profiles.extend(profile_id_from_file_name(&entry.file_name()));
		} else if metadata.is_dir() {
			let folder = entry.file_name().to_string_lossy().into_owned();
			let entries = fs::read_dir(entry.path())?;
			for subentry in entries.flatten() {
				if subentry.metadata()?.is_file()
					&& let Some(id) = profile_id_from_file_name(&subentry.file_name())
				{
					profiles.push(format!("{folder}/{id}"));
				}
			}
		}
	}

	if profiles.is_empty() {
		profiles.push("Default".to_owned());
	}
	profiles.sort_by_key(|profile| profile.to_lowercase());
	profiles.dedup();

	Ok(profiles)
}

/// A singleton object to contain all active Store instances that hold a profile.
pub static PROFILE_STORES: Lazy<RwLock<ProfileStores>> = Lazy::new(|| RwLock::new(ProfileStores { stores: HashMap::new() }));

/// A singleton object to manage Store instances for device configurations.
pub static DEVICE_STORES: Lazy<RwLock<DeviceStores>> = Lazy::new(|| RwLock::new(DeviceStores { stores: HashMap::new() }));

pub struct Locks<'a> {
	#[allow(dead_code)]
	pub device_stores: RwLockReadGuard<'a, DeviceStores>,
	pub profile_stores: RwLockReadGuard<'a, ProfileStores>,
}

pub async fn acquire_locks() -> Locks<'static> {
	let device_stores = DEVICE_STORES.read().await;
	let profile_stores = PROFILE_STORES.read().await;
	Locks { device_stores, profile_stores }
}

pub struct LocksMut<'a> {
	pub device_stores: RwLockWriteGuard<'a, DeviceStores>,
	pub profile_stores: RwLockWriteGuard<'a, ProfileStores>,
}

pub async fn acquire_locks_mut() -> LocksMut<'static> {
	let device_stores = DEVICE_STORES.write().await;
	let profile_stores = PROFILE_STORES.write().await;
	LocksMut { device_stores, profile_stores }
}

pub async fn get_slot<'a>(context: &crate::shared::Context, locks: &'a Locks<'_>) -> Result<&'a Option<crate::shared::ActionInstance>, anyhow::Error> {
	let device = DEVICES.get(&context.device).ok_or_else(|| anyhow!("device not found"))?;
	let store = locks.profile_stores.get_profile_store(&device, &context.profile)?;

	let configured = match &context.controller[..] {
		"Encoder" => store.value.sliders.get(context.position as usize).ok_or_else(|| anyhow!("index out of bounds"))?,
		_ => store.value.keys.get(context.position as usize).ok_or_else(|| anyhow!("index out of bounds"))?,
	};

	Ok(configured)
}

pub async fn get_slot_mut<'a>(context: &crate::shared::Context, locks: &'a mut LocksMut<'_>) -> Result<&'a mut Option<crate::shared::ActionInstance>, anyhow::Error> {
	let device = DEVICES.get(&context.device).ok_or_else(|| anyhow!("device not found"))?;
	let store = locks.profile_stores.get_profile_store_mut(&device, &context.profile).await?;

	let configured = match &context.controller[..] {
		"Encoder" => store.value.sliders.get_mut(context.position as usize).ok_or_else(|| anyhow!("index out of bounds"))?,
		_ => store.value.keys.get_mut(context.position as usize).ok_or_else(|| anyhow!("index out of bounds"))?,
	};

	Ok(configured)
}

pub async fn get_instance<'a>(context: &crate::shared::ActionContext, locks: &'a Locks<'_>) -> Result<Option<&'a crate::shared::ActionInstance>, anyhow::Error> {
	let slot = get_slot(&(context.into()), locks).await?;
	if let Some(instance) = slot {
		if instance.context == *context {
			return Ok(Some(instance));
		} else if let Some(children) = &instance.children {
			for child in children {
				if child.context == *context {
					return Ok(Some(child));
				}
			}
		}
	}
	Ok(None)
}

pub async fn get_instance_mut<'a>(context: &crate::shared::ActionContext, locks: &'a mut LocksMut<'_>) -> Result<Option<&'a mut crate::shared::ActionInstance>, anyhow::Error> {
	let slot = get_slot_mut(&(context.into()), locks).await?;
	if let Some(instance) = slot {
		if instance.context == *context {
			return Ok(Some(instance));
		} else if let Some(children) = &mut instance.children {
			for child in children {
				if child.context == *context {
					return Ok(Some(child));
				}
			}
		}
	}
	Ok(None)
}

pub async fn save_profile(device: &str, locks: &mut LocksMut<'_>) -> Result<(), anyhow::Error> {
	let selected_profile = locks.device_stores.get_selected_profile(device)?;
	let device = DEVICES.get(device).ok_or_else(|| anyhow::anyhow!("Device {device} disconnected while its profile was being saved"))?;
	let store = locks.profile_stores.get_profile_store(&device, &selected_profile)?;
	store.save()
}

#[cfg(test)]
mod tests {
	use super::{DeviceConfig, profile_id_from_file_name, rename_profile_contents};
	use crate::shared::{Action, ActionContext, ActionInstance, ActionState, Profile};
	use std::path::PathBuf;

	#[test]
	fn legacy_device_config_defaults_selected_profile() {
		let config: DeviceConfig = serde_json::from_value(serde_json::json!({
			"selected_profile": "Work"
		}))
		.expect("legacy device config should remain readable");

		assert_eq!(config.selected_profile, "Work");
	}

	#[test]
	fn profile_file_names_support_current_and_recovery_store_artifacts() {
		assert_eq!(profile_id_from_file_name("Default.json".as_ref()).as_deref(), Some("Default"));
		assert_eq!(profile_id_from_file_name("Default.json.bak".as_ref()).as_deref(), Some("Default"));
		assert_eq!(profile_id_from_file_name("Default.json.temp".as_ref()).as_deref(), Some("Default"));
		assert_eq!(profile_id_from_file_name("notes.txt".as_ref()), None);
	}

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
