use crate::shared::config_dir;
use crate::store::Store;

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;

use anyhow::Context;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

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

impl crate::store::NotProfile for DeviceConfig {}

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

/// A singleton object to manage Store instances for device configurations.
pub static DEVICE_STORES: Lazy<RwLock<DeviceStores>> = Lazy::new(|| RwLock::new(DeviceStores { stores: HashMap::new() }));

#[cfg(test)]
mod tests {
	use super::{DeviceConfig, profile_id_from_file_name};

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
}
