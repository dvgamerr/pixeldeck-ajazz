use super::manifest;
use crate::shared::convert_icon;

use std::path;

/// Maps fonts that are unavailable on every platform to bundled equivalents.
fn substitute_font_family(family: &str) -> Option<&'static str> {
	match family.to_lowercase().trim() {
		"arial" => Some("Liberation Sans"),
		"arial black" => Some("Archivo Black"),
		"comic sans ms" => Some("Comic Neue"),
		"courier" | "Courier New" => Some("Courier Prime"),
		"georgia" => Some("Tinos"),
		"impact" => Some("Anton"),
		"microsoft sans serif" | "Times New Roman" => Some("Liberation Serif"),
		"tahoma" | "Verdana" => Some("Open Sans"),
		"trebuchet ms" => Some("Fira Sans"),
		_ => None,
	}
}

/// Resolves icon, property inspector and font paths of the manifest's actions relative to the plugin directory.
pub(super) fn localise_manifest(manifest: &mut manifest::PluginManifest, path: &path::Path, plugin_uuid: &str) {
	if let Some(icon) = manifest.category_icon.take() {
		let category_icon_path = path.join(icon);
		manifest.category_icon = Some(convert_icon(category_icon_path.to_string_lossy().to_string()));
	}

	for action in &mut manifest.actions {
		plugin_uuid.clone_into(&mut action.plugin);

		let action_icon_path = path.join(action.icon.clone());
		action.icon = convert_icon(action_icon_path.to_str().unwrap().to_owned());

		if !action.property_inspector.is_empty() {
			action.property_inspector = path.join(&action.property_inspector).to_string_lossy().to_string();
		} else if let Some(ref property_inspector) = manifest.property_inspector_path {
			action.property_inspector = path.join(property_inspector).to_string_lossy().to_string();
		}

		for state in &mut action.states {
			if state.image == "actionDefaultImage" {
				state.image.clone_from(&action.icon);
			} else {
				let state_icon = path.join(state.image.clone());
				state.image = convert_icon(state_icon.to_str().unwrap().to_owned());
			}

			if let Some(family) = substitute_font_family(&state.family) {
				family.clone_into(&mut state.family);
			}
		}
	}
}
