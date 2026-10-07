mod devices;
mod references;
mod slots;

pub use devices::{DEVICE_STORES, get_device_profiles};
pub use slots::{LocksMut, acquire_locks, acquire_locks_mut, get_instance, get_instance_mut, get_slot_mut, save_profile};

use super::Store;

use crate::shared::{ActionInstance, DeviceInfo, Profile, config_dir};

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, anyhow, bail};
use once_cell::sync::Lazy;
use tokio::sync::RwLock;

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

		references::rename_profile_contents(&mut renamed_profile, new_id, &old_images, &new_images);
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
			references::update_store_references(store, device, old_id, new_id)?;
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

/// A singleton object to contain all active Store instances that hold a profile.
pub static PROFILE_STORES: Lazy<RwLock<ProfileStores>> = Lazy::new(|| RwLock::new(ProfileStores { stores: HashMap::new() }));
