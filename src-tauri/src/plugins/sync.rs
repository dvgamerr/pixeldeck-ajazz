use super::manifest;
use crate::APP_HANDLE;

use std::{fs, path};

use log::{error, warn};
use tauri::Manager;

fn should_sync_builtin_plugin(existing_version: &semver::Version, builtin_version: &semver::Version, development: bool) -> bool {
	development || existing_version < builtin_version
}

fn read_manifest_version(plugin_dir: &path::Path) -> Result<semver::Version, anyhow::Error> {
	let manifest = serde_json::from_slice::<manifest::PluginManifest>(&fs::read(plugin_dir.join("manifest.json"))?)?;
	Ok(semver::Version::parse(&manifest.version)?)
}

fn builtin_plugin_needs_sync(existing_path: &path::Path, builtin_version: &semver::Version) -> bool {
	read_manifest_version(existing_path)
		.map(|existing_version| should_sync_builtin_plugin(&existing_version, builtin_version, cfg!(debug_assertions)))
		.unwrap_or(true)
}

/// Replaces an installed plugin with the bundled copy, restoring the previous one if copying fails.
fn replace_installed_plugin(builtin_path: &path::Path, existing_path: &path::Path) -> Result<(), anyhow::Error> {
	let old_path = existing_path.with_extension("old");
	if existing_path.exists() {
		fs::rename(existing_path, &old_path)?;
	}
	if crate::shared::copy_dir(builtin_path, existing_path).is_err() && old_path.exists() {
		fs::rename(&old_path, existing_path)?;
	}
	let _ = fs::remove_dir_all(&old_path);
	Ok(())
}

fn sync_builtin_plugin(entry: &fs::DirEntry, plugin_dir: &path::Path) -> Result<(), anyhow::Error> {
	let builtin_version = read_manifest_version(&entry.path())?;
	let existing_path = plugin_dir.join(entry.file_name());
	if builtin_plugin_needs_sync(&existing_path, &builtin_version) {
		replace_installed_plugin(&entry.path(), &existing_path)?;
	}
	Ok(())
}

/// Copies bundled plugins into the plugins directory when they are missing or outdated.
pub(super) fn sync_builtin_plugins(plugin_dir: &path::Path) {
	let Ok(Ok(entries)) = APP_HANDLE.get().unwrap().path().resolve("plugins", tauri::path::BaseDirectory::Resource).map(fs::read_dir) else {
		return;
	};
	for entry in entries.flatten() {
		if let Err(error) = sync_builtin_plugin(&entry, plugin_dir) {
			error!("Failed to upgrade builtin plugin {}: {}", entry.file_name().to_string_lossy(), error);
		}
	}
}

/// Initialises every plugin directory found in the plugins folder.
pub(super) fn spawn_installed_plugins(plugin_dir: &path::Path) {
	let entries = match fs::read_dir(plugin_dir) {
		Ok(p) => p,
		Err(error) => {
			error!("Failed to read plugins directory at {}: {}", plugin_dir.display(), error);
			panic!()
		}
	};

	for entry in entries {
		let entry = match entry {
			Ok(entry) => entry,
			Err(error) => {
				warn!("Failed to read entry of plugins directory: {}", error);
				continue;
			}
		};
		let path = match entry.metadata().unwrap().is_symlink() {
			true => fs::read_link(entry.path()).unwrap(),
			false => entry.path(),
		};
		if !fs::metadata(&path).unwrap().is_dir() {
			continue;
		}
		tokio::spawn(async move {
			if let Err(error) = super::initialise_plugin(&path).await {
				warn!("Failed to initialise plugin at {}: {:#}", path.display(), error);
			}
		});
	}
}

#[cfg(test)]
mod tests {
	use super::should_sync_builtin_plugin;
	use semver::Version;

	#[test]
	fn development_builds_always_sync_builtin_plugins() {
		let builtin = Version::new(2, 12, 3);

		assert!(should_sync_builtin_plugin(&Version::new(2, 12, 3), &builtin, true));
		assert!(should_sync_builtin_plugin(&Version::new(3, 0, 0), &builtin, true));
	}

	#[test]
	fn release_builds_only_upgrade_older_builtin_plugins() {
		let builtin = Version::new(2, 12, 3);

		assert!(should_sync_builtin_plugin(&Version::new(2, 12, 2), &builtin, false));
		assert!(!should_sync_builtin_plugin(&Version::new(2, 12, 3), &builtin, false));
		assert!(!should_sync_builtin_plugin(&Version::new(3, 0, 0), &builtin, false));
	}
}
