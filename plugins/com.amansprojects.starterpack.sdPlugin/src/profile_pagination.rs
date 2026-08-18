use crate::compat::{DialRotateEvent, EventHandlerResult, OutboundEventManager, SettingsValue};

use crate::{
	pixel::{data_uri, text_path},
	switch_profile,
};

pub const ACTION: &str = "com.amansprojects.starterpack.profilepagination";

#[derive(Debug, Default, PartialEq, Eq)]
struct PaginationSettings {
	profiles: Vec<String>,
}

impl PaginationSettings {
	fn from_settings(settings: &SettingsValue) -> Self {
		let mut profiles = Vec::new();
		if let Some(configured) = settings
			.as_object()
			.and_then(|settings| settings.get("profiles"))
			.and_then(SettingsValue::as_array)
		{
			for profile in configured.iter().filter_map(SettingsValue::as_str) {
				if !profile.is_empty() && !profiles.iter().any(|existing| existing == profile) {
					profiles.push(profile.to_owned());
				}
			}
		}
		Self { profiles }
	}
}

fn context_profile(context: &str) -> Option<&str> {
	context.split('.').nth(1)
}

fn selected_page(context: &str, profiles: &[String]) -> usize {
	context_profile(context)
		.and_then(|current| profiles.iter().position(|profile| profile == current))
		.unwrap_or_default()
}

fn rotated_page(current: usize, count: usize, ticks: i16) -> usize {
	if count == 0 {
		return 0;
	}
	let delta = isize::from(ticks).rem_euclid(count as isize) as usize;
	(current + delta) % count
}

fn shortened_profile(profile: &str) -> String {
	let profile = profile.rsplit('/').next().unwrap_or(profile);
	let mut characters = profile.chars();
	let shortened = characters.by_ref().take(14).collect::<String>();
	if characters.next().is_some() {
		format!("{shortened}...")
	} else {
		shortened
	}
}

fn pagination_image(page: usize, profiles: &[String]) -> String {
	let count = profiles.len();
	let profile = if count == 0 {
		"NO PROFILE".to_owned()
	} else {
		shortened_profile(&profiles[page])
	};
	let profile_size = match profile.chars().count() {
		0..=8 => 17,
		9..=12 => 15,
		_ => 13,
	};
	let profile_color = if count == 0 { "#52525b" } else { "#f4f4f5" };
	let profile = text_path(&profile, 88.0, 59, profile_size, profile_color);

	let (indicator_size, indicator_gap) = if count <= 10 {
		(8.0, 6.0)
	} else if count <= 14 {
		(7.0, 4.0)
	} else {
		let size = ((120.0 - 2.0 * count.saturating_sub(1) as f32) / count as f32).clamp(2.0, 5.0);
		(size, 2.0)
	};
	let indicator_width =
		indicator_size * count as f32 + indicator_gap * count.saturating_sub(1) as f32;
	let indicator_start = (176.0 - indicator_width) / 2.0;
	let indicators = (0..count)
		.map(|index| {
			let x = indicator_start + (indicator_size + indicator_gap) * index as f32;
			let fill = if index == page { "#ffffff" } else { "#3f3f46" };
			format!(
				r##"<rect x="{x:.2}" y="77" width="{indicator_size:.2}" height="{indicator_size:.2}" fill="{fill}"/>"##
			)
		})
		.collect::<String>();

	data_uri(&format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" width="176" height="112" viewBox="0 0 176 112" shape-rendering="crispEdges">
<rect width="176" height="112" fill="#070708"/>
{profile}{indicators}
</svg>"##
	))
}

async fn render(
	context: String,
	settings: &SettingsValue,
	outbound: &mut OutboundEventManager,
) -> EventHandlerResult {
	let settings = PaginationSettings::from_settings(settings);
	let page = selected_page(&context, &settings.profiles);
	outbound
		.set_image(
			context,
			Some(pagination_image(page, &settings.profiles)),
			None,
		)
		.await?;
	Ok(())
}

pub async fn appear(
	context: String,
	settings: SettingsValue,
	outbound: &mut OutboundEventManager,
) -> EventHandlerResult {
	render(context, &settings, outbound).await
}

pub async fn refresh(
	context: String,
	settings: SettingsValue,
	outbound: &mut OutboundEventManager,
) -> EventHandlerResult {
	render(context, &settings, outbound).await
}

pub async fn rotate(
	event: DialRotateEvent,
	outbound: &mut OutboundEventManager,
) -> EventHandlerResult {
	let settings = PaginationSettings::from_settings(&event.payload.settings);
	if settings.profiles.is_empty() {
		outbound.show_alert(event.context.clone()).await?;
		return render(event.context, &event.payload.settings, outbound).await;
	}

	let current = selected_page(&event.context, &settings.profiles);
	let page = rotated_page(current, settings.profiles.len(), event.payload.ticks);
	outbound
		.set_image(
			event.context.clone(),
			Some(pagination_image(page, &settings.profiles)),
			None,
		)
		.await?;

	let target = &settings.profiles[page];
	if context_profile(&event.context) != Some(target.as_str()) {
		switch_profile::switch(event.device, target.clone(), outbound).await?;
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::{PaginationSettings, pagination_image, rotated_page, selected_page};

	#[test]
	fn settings_keep_checked_profile_order_and_remove_duplicates() {
		let settings = PaginationSettings::from_settings(&serde_json::json!({
			"profiles": ["Work", "Gaming", "Work", ""]
		}));
		assert_eq!(settings.profiles, ["Work", "Gaming"]);
	}

	#[test]
	fn current_profile_selects_page_and_rotation_wraps() {
		let profiles = vec!["One".to_owned(), "Two".to_owned(), "Three".to_owned()];
		assert_eq!(selected_page("sd-device.Two.Encoder.0.0", &profiles), 1);
		assert_eq!(rotated_page(1, profiles.len(), 1), 2);
		assert_eq!(rotated_page(2, profiles.len(), 1), 0);
		assert_eq!(rotated_page(0, profiles.len(), -1), 2);
	}

	#[test]
	fn renderer_outputs_touchscreen_sized_svg() {
		let image = pagination_image(1, &["One".to_owned(), "Two".to_owned(), "Three".to_owned()]);
		assert!(image.starts_with("data:image/svg+xml,"));
		assert!(image.contains("width%3D%22176%22"));
		assert!(image.contains("height%3D%22112%22"));
		assert!(!image.contains("stroke"));
		assert_eq!(image.matches("%3Crect").count(), 4);
	}
}
