//! SVG text helpers shared by the plugin crates through `#[path]`.
//! Each plugin supplies its own font face; everything else is identical.

use std::fmt::Write;
use ttf_parser::{Face, OutlineBuilder};

struct GlyphPath {
	data: String,
	x: f32,
}

impl GlyphPath {
	/// Appends one path command with the glyph x offset applied to every x coordinate.
	fn command(&mut self, op: char, points: &[(f32, f32)]) {
		self.data.push(op);
		for (index, (x, y)) in points.iter().enumerate() {
			if index > 0 {
				self.data.push(' ');
			}
			let _ = write!(self.data, "{} {}", (x + self.x) as i32, *y as i32);
		}
	}
}

impl OutlineBuilder for GlyphPath {
	fn move_to(&mut self, x: f32, y: f32) {
		self.command('M', &[(x, y)]);
	}

	fn line_to(&mut self, x: f32, y: f32) {
		self.command('L', &[(x, y)]);
	}

	fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
		self.command('Q', &[(x1, y1), (x, y)]);
	}

	// The argument list is dictated by the `OutlineBuilder` trait.
	fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
		self.command('C', &[(x1, y1), (x2, y2), (x, y)]);
	}

	fn close(&mut self) {
		self.data.push('Z');
	}
}

fn glyph_for(face: &Face<'_>, character: char) -> Option<ttf_parser::GlyphId> {
	face.glyph_index(character)
		.or_else(|| face.glyph_index('?'))
}

#[derive(Clone, Copy)]
pub struct Placement {
	pub center: f32,
	pub baseline: u8,
	pub size: u8,
}

pub fn text_path(face: &Face<'_>, text: &str, placement: Placement, fill: &str) -> String {
	let Placement {
		center,
		baseline,
		size,
	} = placement;
	let scale = f32::from(size) / f32::from(face.units_per_em());
	let mut path = GlyphPath {
		data: String::with_capacity(text.len() * 80),
		x: 0.0,
	};
	for glyph in text
		.chars()
		.filter_map(|character| glyph_for(face, character))
	{
		face.outline_glyph(glyph, &mut path);
		path.x += f32::from(face.glyph_hor_advance(glyph).unwrap_or_default());
	}
	let x = center - path.x * scale / 2.0;
	format!(
		r##"<path d="{}" transform="translate({x:.2} {baseline}) scale({scale:.5} -{scale:.5})" fill="{fill}"/>"##,
		path.data
	)
}

#[allow(dead_code)]
pub fn text_width(face: &Face<'_>, text: &str, size: u8) -> f32 {
	let advance = text
		.chars()
		.filter_map(|character| glyph_for(face, character))
		.map(|glyph| f32::from(face.glyph_hor_advance(glyph).unwrap_or_default()))
		.sum::<f32>();
	advance * f32::from(size) / f32::from(face.units_per_em())
}

pub fn data_uri(svg: &str) -> String {
	let mut encoded = String::with_capacity(svg.len() * 2);
	for byte in svg.bytes() {
		if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
			encoded.push(byte as char);
		} else {
			let _ = write!(encoded, "%{byte:02X}");
		}
	}
	format!("data:image/svg+xml,{encoded}")
}
