use std::sync::LazyLock;

#[path = "../../../shared/svg_text.rs"]
mod svg_text;

pub use svg_text::data_uri;

static PIXELOID: LazyLock<ttf_parser::Face<'static>> = LazyLock::new(|| {
	ttf_parser::Face::parse(include_bytes!("../assets/fonts/PixeloidSans.ttf"), 0).unwrap()
});

pub fn text_path(text: &str, center: f32, baseline: u8, size: u8, fill: &str) -> String {
	svg_text::text_path(
		&PIXELOID,
		text,
		svg_text::Placement {
			center,
			baseline,
			size,
		},
		fill,
	)
}

pub fn text_width(text: &str, size: u8) -> f32 {
	svg_text::text_width(&PIXELOID, text, size)
}
