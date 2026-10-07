use base64::Engine as _;
use image::ImageFormat;
use serde::{Deserialize, Serialize};
use tauri::command;

use super::Error;
use crate::store::{NotProfile, Store};

const MAX_STARTUP_IMAGE_BYTES: usize = 10 * 1024 * 1024;
const MAX_STARTUP_IMAGE_LAYERS: usize = 64;

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct StartupImageProject {
	pub layers: Vec<StartupImageLayer>,
}

impl NotProfile for StartupImageProject {}

#[derive(Clone, Deserialize, Serialize)]
pub struct StartupImageLayer {
	pub id: String,
	pub name: String,
	pub image: String,
	pub zoom: f64,
	pub offset_x: f64,
	pub offset_y: f64,
	pub rotation: f64,
}

fn startup_image_project_store(device: &str) -> Result<Store<StartupImageProject>, anyhow::Error> {
	let id = format!("startup-image-{}", urlencoding::encode(device));
	Store::new(&id, &crate::shared::config_dir().join("startup-images"), StartupImageProject::default())
}

fn layer_transform_is_valid(layer: &StartupImageLayer) -> bool {
	[layer.zoom, layer.offset_x, layer.offset_y, layer.rotation].iter().all(|value| value.is_finite()) && (0.25..=3.0).contains(&layer.zoom)
}

fn validate_startup_image_project(project: &StartupImageProject) -> Result<(), anyhow::Error> {
	if project.layers.len() > MAX_STARTUP_IMAGE_LAYERS {
		return Err(anyhow::anyhow!("A startup image can contain at most {MAX_STARTUP_IMAGE_LAYERS} layers"));
	}

	for layer in &project.layers {
		if layer.id.trim().is_empty() || layer.name.trim().is_empty() {
			return Err(anyhow::anyhow!("Every startup image layer must have an ID and name"));
		}
		if !layer_transform_is_valid(layer) {
			return Err(anyhow::anyhow!("A startup image layer contains an invalid transform"));
		}
		validate_project_image(&layer.image)?;
	}

	Ok(())
}

fn decode_image_data_url(value: &str) -> Result<(&str, Vec<u8>), anyhow::Error> {
	let (metadata, encoded) = value.split_once(',').ok_or_else(|| anyhow::anyhow!("The selected image is invalid"))?;

	if encoded.len() > MAX_STARTUP_IMAGE_BYTES.saturating_mul(4).div_ceil(3) + 4 {
		return Err(anyhow::anyhow!("The selected image must be 10 MB or smaller"));
	}

	let bytes = base64::engine::general_purpose::STANDARD.decode(encoded)?;
	if bytes.len() > MAX_STARTUP_IMAGE_BYTES {
		return Err(anyhow::anyhow!("The selected image must be 10 MB or smaller"));
	}

	Ok((metadata, bytes))
}

fn decode_startup_image(value: &str) -> Result<image::DynamicImage, anyhow::Error> {
	let (metadata, bytes) = decode_image_data_url(value)?;
	if !matches!(metadata, "data:image/png;base64" | "data:image/jpeg;base64" | "data:image/bmp;base64" | "data:image/x-ms-bmp;base64") {
		return Err(anyhow::anyhow!("Only PNG, JPG, JPEG, and BMP images can be sent directly to the device"));
	}

	let format = image::guess_format(&bytes)?;
	if !matches!(format, ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Bmp) {
		return Err(anyhow::anyhow!("Only PNG, JPG, JPEG, and BMP images are supported"));
	}

	Ok(image::load_from_memory_with_format(&bytes, format)?)
}

fn compact_lowercase(value: &str) -> String {
	value
		.to_ascii_lowercase()
		.chars()
		.filter(|character| !character.is_whitespace() && *character != '\'' && *character != '"')
		.collect()
}

fn has_external_reference(compact: &str) -> bool {
	compact.contains("@import") || compact.contains("url(http") || compact.contains("url(//")
}

fn validate_svg_element(node: roxmltree::Node) -> Result<(), anyhow::Error> {
	let tag = node.tag_name().name();
	if ["script", "foreignObject", "iframe", "object", "embed"].iter().any(|blocked| tag.eq_ignore_ascii_case(blocked)) {
		return Err(anyhow::anyhow!("The selected SVG contains unsupported active content"));
	}
	if tag.eq_ignore_ascii_case("style") {
		let compact_text = compact_lowercase(node.text().unwrap_or_default());
		if has_external_reference(&compact_text) || compact_text.contains("javascript:") {
			return Err(anyhow::anyhow!("The selected SVG contains an unsupported external reference"));
		}
	}
	Ok(())
}

fn validate_svg_attribute(attribute: roxmltree::Attribute) -> Result<(), anyhow::Error> {
	let name = attribute.name();
	let value = attribute.value().trim();
	let lower_value = value.to_ascii_lowercase();
	if name.to_ascii_lowercase().starts_with("on") || lower_value.contains("javascript:") {
		return Err(anyhow::anyhow!("The selected SVG contains unsupported active content"));
	}

	let is_safe_href_target = value.starts_with('#')
		|| matches!(
			lower_value.split_once(',').map(|(metadata, _)| metadata),
			Some("data:image/png;base64") | Some("data:image/jpeg;base64") | Some("data:image/bmp;base64") | Some("data:image/x-ms-bmp;base64")
		);
	if (name.eq_ignore_ascii_case("href") && !is_safe_href_target) || has_external_reference(&compact_lowercase(&lower_value)) {
		return Err(anyhow::anyhow!("The selected SVG contains an unsupported external reference"));
	}
	Ok(())
}

fn validate_svg(bytes: &[u8]) -> Result<(), anyhow::Error> {
	let source = std::str::from_utf8(bytes)?;
	let document = roxmltree::Document::parse(source)?;
	let root = document.root_element();
	if !root.tag_name().name().eq_ignore_ascii_case("svg") {
		return Err(anyhow::anyhow!("The selected SVG does not contain an SVG root element"));
	}

	for node in document.descendants().filter(|node| node.is_element()) {
		validate_svg_element(node)?;
		for attribute in node.attributes() {
			validate_svg_attribute(attribute)?;
		}
	}

	Ok(())
}

fn validate_project_image(value: &str) -> Result<(), anyhow::Error> {
	let (metadata, bytes) = decode_image_data_url(value)?;
	if metadata == "data:image/svg+xml;base64" {
		validate_svg(&bytes)
	} else {
		decode_startup_image(value).map(|_| ())
	}
}

#[command]
pub async fn set_startup_image(device: &str, image: &str) -> Result<(), Error> {
	let image = decode_startup_image(image)?;
	crate::ajazz::set_startup_image(device, image).await?;
	Ok(())
}

#[command]
pub async fn get_startup_image_project(device: &str) -> Result<StartupImageProject, Error> {
	Ok(startup_image_project_store(device)?.value)
}

#[command]
pub async fn save_startup_image_project(device: &str, project: StartupImageProject) -> Result<(), Error> {
	validate_startup_image_project(&project)?;
	let mut store = startup_image_project_store(device)?;
	store.value = project;
	Ok(store.save()?)
}

#[cfg(test)]
mod tests {
	use base64::Engine as _;

	use super::{StartupImageLayer, StartupImageProject, decode_startup_image, validate_project_image, validate_startup_image_project};

	#[test]
	fn rejects_gif_data_urls() {
		let error = decode_startup_image("data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==").expect_err("GIF must not be accepted");
		assert!(error.to_string().contains("Only PNG, JPG, JPEG, and BMP"));
	}

	#[test]
	fn rejects_gif_disguised_as_png() {
		let error = decode_startup_image("data:image/png;base64,R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==").expect_err("GIF content must not be accepted");
		assert!(error.to_string().contains("Only PNG, JPG, JPEG, and BMP"));
	}

	#[test]
	fn accepts_safe_svg_project_image() {
		let encoded = base64::engine::general_purpose::STANDARD.encode(r##"<svg xmlns="http://www.w3.org/2000/svg"><rect width="10" height="10" fill="#fff"/></svg>"##);
		validate_project_image(&format!("data:image/svg+xml;base64,{encoded}")).expect("Safe SVG must be accepted");
	}

	#[test]
	fn rejects_active_svg_content() {
		let encoded = base64::engine::general_purpose::STANDARD.encode(r#"<svg xmlns="http://www.w3.org/2000/svg" onload="alert(1)"><script>alert(1)</script></svg>"#);
		let error = validate_project_image(&format!("data:image/svg+xml;base64,{encoded}")).expect_err("Active SVG content must not be accepted");
		assert!(error.to_string().contains("active content"));
	}

	#[test]
	fn rejects_invalid_layer_transform() {
		let project = StartupImageProject {
			layers: vec![StartupImageLayer {
				id: "layer-1".to_owned(),
				name: "Layer".to_owned(),
				image: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=".to_owned(),
				zoom: 4.0,
				offset_x: 0.0,
				offset_y: 0.0,
				rotation: 0.0,
			}],
		};

		let error = validate_startup_image_project(&project).expect_err("Zoom outside the editor range must not be accepted");
		assert!(error.to_string().contains("invalid transform"));
	}
}
