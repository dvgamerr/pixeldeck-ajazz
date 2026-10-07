use super::{
	ANIMATION_INTERVAL, AudioSnapshot, centered_status_layout, device_switch_indicator,
	driver_name, snapshot_image, text_width,
};

#[test]
fn audio_animation_is_rate_limited_for_hid_displays() {
	assert_eq!(ANIMATION_INTERVAL, std::time::Duration::from_secs(1));
}

#[test]
fn speaker_and_status_are_centered_as_one_group() {
	for status in ["0%", "50%", "100%", "MUTED"] {
		let (speaker_offset, status_center) = centered_status_layout(status);
		let speaker_left = 10.0 + speaker_offset;
		let status_right = status_center + text_width(status, 29) / 2.0;
		assert!(((speaker_left + status_right) / 2.0 - 88.0).abs() < 0.01);
	}
}

#[test]
fn device_switch_uses_directional_chevrons_instead_of_text() {
	assert!(device_switch_indicator("PREV").contains("M14 95"));
	assert!(device_switch_indicator("NEXT").contains("M162 95"));
	assert!(!device_switch_indicator("PREV").contains("PREV"));
	assert!(!device_switch_indicator("NEXT").contains("NEXT"));
}

#[test]
fn lcd_image_uses_native_zone_dimensions_and_font_paths() {
	let image = snapshot_image(
		&AudioSnapshot {
			device_name: "Speakers (Realtek Audio)".to_owned(),
			volume: 67,
			muted: false,
			peak: 60,
		},
		"",
	);

	assert!(image.starts_with("data:image/svg+xml,"));
	assert!(image.contains("width%3D%22176%22"));
	assert!(image.contains("height%3D%22112%22"));
	assert!(image.contains("translate%280%20-13%29"));
	assert!(image.contains("transform%3D%22translate"));
	assert!(!image.contains("%3Ctext"));
	assert!(image.len() < 50_000);
}

#[test]
fn output_type_is_removed_from_device_name() {
	assert_eq!(
		driver_name("Speakers (Realtek(R) Audio)"),
		"Realtek(R) Audio"
	);
	assert_eq!(driver_name("Headphones - WH-1000XM5"), "WH-1000XM5");
	assert_eq!(
		driver_name("NVIDIA High Definition Audio"),
		"NVIDIA High Definition Audio"
	);
	assert_eq!(driver_name("Speakerphone USB"), "Speakerphone USB");
}

#[test]
fn lcd_image_marks_muted_and_device_switch_states() {
	let muted = snapshot_image(
		&AudioSnapshot {
			device_name: "Output".to_owned(),
			volume: 50,
			muted: true,
			peak: 80,
		},
		"",
	);
	let switched = snapshot_image(
		&AudioSnapshot {
			device_name: "Output".to_owned(),
			volume: 50,
			muted: false,
			peak: 80,
		},
		"NEXT",
	);

	assert!(muted.contains("%23ff3155"));
	assert!(switched.contains("%23facc15"));
	assert!(!switched.contains("NEXT"));
	assert!(!switched.contains("PREV"));
	assert!(switched.contains("%23000"));
	assert!(!switched.contains("linearGradient"));
	assert!(!switched.contains("rx%3D"));
}

#[test]
fn real_audio_peak_changes_the_pixel_speaker_frame() {
	let quiet = snapshot_image(
		&AudioSnapshot {
			device_name: "Output".to_owned(),
			volume: 50,
			muted: false,
			peak: 0,
		},
		"",
	);
	let loud = snapshot_image(
		&AudioSnapshot {
			device_name: "Output".to_owned(),
			volume: 50,
			muted: false,
			peak: 90,
		},
		"",
	);

	assert_ne!(quiet, loud);
	assert!(!quiet.contains("M52%2032"));
	assert!(loud.contains("M52%2032"));
}

#[test]
fn audio_peak_animation_keeps_the_speaker_anchor_fixed() {
	let (speaker_offset, _) = centered_status_layout("50%");
	let expected_transform =
		format!("%3Cg%20transform%3D%22translate%28{speaker_offset:.2}%200%29%22%3E");

	for peak in [0, 10, 30, 90] {
		let image = snapshot_image(
			&AudioSnapshot {
				device_name: "Output".to_owned(),
				volume: 50,
				muted: false,
				peak,
			},
			"",
		);

		assert!(
			image.contains(&expected_transform),
			"speaker moved at peak {peak}"
		);
	}
}

#[cfg(windows)]
#[test]
fn windows_default_output_snapshot_is_readable() {
	let snapshot = super::platform::snapshot().expect("default Windows output should be readable");

	assert!(!snapshot.device_name.trim().is_empty());
	assert!(snapshot.volume <= 100);
}
