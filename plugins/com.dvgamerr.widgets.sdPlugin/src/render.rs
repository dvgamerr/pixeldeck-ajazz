use chrono::{Datelike, Local, Timelike, Weekday};
use serde_json::Value as SettingsValue;

use crate::{
	model::{
		ActionKind, AirQualityData, CurrencyData, GoldData, StockData, WeatherData, WidgetData,
		setting_i64, setting_string,
	},
	pixel::{data_uri, text_path},
};

const CYAN: &str = "#20e3ff";
const MAGENTA: &str = "#ff4fd8";
const YELLOW: &str = "#facc15";
const GREEN: &str = "#22c55e";
const RED: &str = "#ff3155";
const WHITE: &str = "#f8fafc";
const MUTED: &str = "#8c9aaa";

fn shell(accent: &str, content: &str) -> String {
	data_uri(&format!(
		r##"<svg xmlns="http://www.w3.org/2000/svg" width="144" height="144" viewBox="0 0 144 144" shape-rendering="crispEdges">
<rect width="144" height="144" fill="#000"/>
<path d="M9 9h126v5H9z" fill="{accent}"/>
{content}
</svg>"##
	))
}

fn clipped(value: &str, max: usize) -> String {
	let mut characters = value.trim().chars();
	let mut result: String = characters.by_ref().take(max).collect();
	if characters.next().is_some() && max > 1 {
		result.pop();
		result.push('~');
	}
	result
}

fn signed(value: f64, decimals: usize) -> String {
	format!("{value:+.*}%", decimals)
}

fn number_2(value: f64) -> String {
	let value = if value.abs() < 0.005 { 0.0 } else { value };
	let formatted = format!("{value:.2}");
	let (integer, decimals) = formatted.split_once('.').unwrap_or((&formatted, "00"));
	let (sign, digits) = integer
		.strip_prefix('-')
		.map_or(("", integer), |digits| ("-", digits));
	let mut reversed = String::with_capacity(digits.len() + digits.len() / 3);
	for (index, character) in digits.chars().rev().enumerate() {
		if index > 0 && index % 3 == 0 {
			reversed.push(',');
		}
		reversed.push(character);
	}
	let grouped: String = reversed.chars().rev().collect();
	format!("{sign}{grouped}.{decimals}")
}

fn signed_number_2(value: f64) -> String {
	if value >= 0.0 {
		format!("+{}", number_2(value))
	} else {
		number_2(value)
	}
}

pub fn loading(kind: ActionKind) -> String {
	let label = match kind {
		ActionKind::Gold => "GOLD",
		ActionKind::Currency => "FX RATE",
		ActionKind::Stock => "STOCK",
		ActionKind::AirQuality => "AIR QUALITY",
		ActionKind::Weather => "WEATHER",
		ActionKind::WorkHours => "WORK CLOCK",
		_ => "WIDGET",
	};
	let content = format!(
		"{}{}<path d=\"M42 91h60\" stroke=\"{CYAN}\" stroke-width=\"5\" stroke-dasharray=\"8 6\"/>",
		text_path(label, 72.0, 62, 14, WHITE),
		text_path("LOADING", 72.0, 116, 10, MUTED),
	);
	shell(CYAN, &content)
}

pub fn error(message: &str) -> String {
	let label = "DATA ERROR";
	let content = format!(
		"{}{}{}",
		text_path("!", 72.0, 70, 44, RED),
		text_path(label, 72.0, 101, 13, RED),
		text_path(&clipped(message, 22).to_uppercase(), 72.0, 122, 7, MUTED),
	);
	shell(RED, &content)
}

pub fn widget(kind: ActionKind, data: &WidgetData, settings: &SettingsValue) -> String {
	match (kind, data) {
		(ActionKind::Gold, WidgetData::Gold(data)) => gold(data, settings),
		(ActionKind::Currency, WidgetData::Currency(data)) => currency(data),
		(ActionKind::Stock, WidgetData::Stock(data)) => stock(data),
		(ActionKind::AirQuality, WidgetData::AirQuality(data)) => air_quality(data),
		(ActionKind::Weather, WidgetData::Weather(data)) => weather(data),
		(ActionKind::WorkHours, WidgetData::WorkHours) => work_hours(settings),
		_ => error("invalid response"),
	}
}

fn gold(data: &GoldData, settings: &SettingsValue) -> String {
	let currency = setting_string(settings, "currency", "USD").to_uppercase();
	let price = if currency == "THB" {
		data.price_usd * data.exchange_rate
	} else {
		data.price_usd
	};
	let value = number_2(price);
	let size = if value.len() > 9 { 22 } else { 29 };
	let content = format!(
		"{}{}{}{}",
		text_path(&format!("GOLD {currency}"), 72.0, 36, 13, YELLOW),
		text_path(&value, 72.0, 83, size, WHITE),
		text_path("SPOT / TROY OZ", 72.0, 107, 9, MUTED),
		text_path("PRESS: XAUUSD", 72.0, 126, 7, "#586576"),
	);
	shell(YELLOW, &content)
}

fn currency(data: &CurrencyData) -> String {
	let accent = if data.change_percent >= 0.0 {
		GREEN
	} else {
		RED
	};
	let value = number_2(data.price);
	let content = format!(
		"{}{}{}{}",
		text_path(&data.pair, 72.0, 37, 15, CYAN),
		text_path(&value, 72.0, 82, 29, WHITE),
		text_path(&signed(data.change_percent, 2), 72.0, 111, 15, accent),
		text_path("PRESS TO REFRESH", 72.0, 129, 7, "#586576"),
	);
	shell(CYAN, &content)
}

fn stock(data: &StockData) -> String {
	let accent = if data.change_percent >= 0.0 {
		GREEN
	} else {
		RED
	};
	let title = if data.display_name.is_empty() {
		data.symbol.clone()
	} else {
		clipped(&data.display_name.to_uppercase(), 16)
	};
	let secondary = data
		.profit_loss
		.map(|value| format!("P/L {}", signed_number_2(value)))
		.unwrap_or_else(|| signed(data.change_percent, 2));
	let open = data
		.open
		.map(|value| format!("OPEN {}", number_2(value)))
		.unwrap_or_else(|| "OPEN --".to_owned());
	let content = format!(
		"{}{}{}{}{}",
		text_path(&title, 72.0, 32, 13, accent),
		text_path(&number_2(data.price), 72.0, 73, 28, WHITE),
		text_path(&open, 72.0, 96, 9, MUTED),
		text_path(&secondary, 72.0, 118, 13, accent),
		text_path("YAHOO FINANCE", 72.0, 132, 6, "#586576"),
	);
	shell(accent, &content)
}

fn aqi_style(data: &AirQualityData) -> (&'static str, &'static str) {
	match data.aqi {
		0..=50 => (GREEN, "GOOD"),
		51..=100 => (YELLOW, "MODERATE"),
		101..=150 => ("#f97316", "SENSITIVE"),
		151..=200 => (RED, "UNHEALTHY"),
		201..=300 => ("#a855f7", "VERY UNHEALTHY"),
		_ => ("#7f1d1d", "HAZARDOUS"),
	}
}

fn air_quality(data: &AirQualityData) -> String {
	let (accent, fallback) = aqi_style(data);
	let level = if data.level.trim().is_empty() {
		fallback.to_owned()
	} else {
		clipped(&data.level.to_uppercase(), 20)
	};
	let content = format!(
		"{}{}{}{}",
		text_path("US AQI", 72.0, 35, 13, MUTED),
		text_path(&data.aqi.to_string(), 72.0, 91, 45, accent),
		text_path(&level, 72.0, 117, 11, accent),
		text_path("PRESS TO REFRESH", 72.0, 132, 6, "#586576"),
	);
	shell(accent, &content)
}

fn weather_description(code: u8) -> &'static str {
	match code {
		0 => "CLEAR",
		1..=3 => "CLOUDY",
		45 | 48 => "FOG",
		51..=67 | 80..=82 => "RAIN",
		71..=77 | 85 | 86 => "SNOW",
		95..=99 => "STORM",
		_ => "WEATHER",
	}
}

fn weather_icon(data: &WeatherData) -> String {
	let sky = if data.is_day { YELLOW } else { "#a78bfa" };
	let precipitation = matches!(data.code, 51..=67 | 80..=82 | 95..=99);
	let cloud = matches!(data.code, 1..=3 | 45 | 48 | 51..=99);
	let mut icon = format!(
		r##"<circle cx="43" cy="57" r="17" fill="none" stroke="{sky}" stroke-width="6"/>"##
	);
	if cloud {
		icon.push_str(r##"<path d="M37 78h55c13 0 13-19 1-20-4-17-29-18-35-3-14-4-25 5-21 23z" fill="#172d3a" stroke="#20e3ff" stroke-width="5"/>"##);
	}
	if precipitation {
		icon.push_str(
			r##"<path d="M49 88l-5 10M67 88l-5 10M85 88l-5 10" stroke="#20e3ff" stroke-width="4"/>"##,
		);
	}
	icon
}

fn weather(data: &WeatherData) -> String {
	let description = weather_description(data.code);
	let accent = if matches!(data.code, 95..=99) {
		MAGENTA
	} else {
		CYAN
	};
	let content = format!(
		"{}{}{}{}{}",
		weather_icon(data),
		text_path(&format!("{} C", data.temperature), 105.0, 54, 22, WHITE),
		text_path(&format!("FEELS {} C", data.apparent), 105.0, 73, 8, MUTED),
		text_path(description, 72.0, 116, 13, accent),
		text_path(
			&format!("RAIN {}%", data.precipitation),
			72.0,
			132,
			7,
			"#586576"
		),
	);
	shell(accent, &content)
}

fn work_hours(settings: &SettingsValue) -> String {
	let start = setting_i64(settings, "startH", 9).clamp(0, 23) as u32;
	let end = setting_i64(settings, "endH", 18).clamp(0, 23) as u32;
	let now = Local::now();
	let weekday = !matches!(now.weekday(), Weekday::Sat | Weekday::Sun);
	let minutes = now.hour() * 60 + now.minute();
	let working = weekday && start < end && minutes >= start * 60 && minutes < end * 60;
	let accent = if working { "#ff9b57" } else { MUTED };
	let progress = if start < end {
		(minutes.saturating_sub(start * 60) as f64 / ((end - start) * 60) as f64).clamp(0.0, 1.0)
	} else {
		0.0
	};
	let width = 112.0 * progress;
	let day = now.format("%a").to_string().to_uppercase();
	let content = format!(
		r##"{}{}{}{}
<rect x="16" y="109" width="112" height="8" fill="#252c37"/>
<rect x="16" y="109" width="{width:.1}" height="8" fill="{accent}"/>
{}"##,
		text_path(if working { "WORK" } else { "REST" }, 72.0, 33, 12, accent),
		text_path(&now.format("%H:%M").to_string(), 72.0, 80, 34, WHITE),
		text_path(&day, 72.0, 100, 10, MUTED),
		text_path(&format!("{start:02}:00"), 29.0, 130, 7, "#586576"),
		text_path(&format!("{end:02}:00"), 115.0, 130, 7, "#586576"),
	);
	shell(accent, &content)
}

pub fn powershell(settings: &SettingsValue, status: &str) -> String {
	let script = setting_string(settings, "label", "");
	let script = if script.is_empty() {
		setting_string(settings, "script", "SCRIPT")
	} else {
		script
	};
	let (accent, badge) = match status {
		"running" => (YELLOW, "RUNNING"),
		"ok" => (GREEN, "COMPLETE"),
		"error" => (RED, "FAILED"),
		_ => (CYAN, "READY"),
	};
	let content = format!(
		"{}{}{}{}",
		text_path("PS >", 72.0, 39, 17, accent),
		text_path(&clipped(&script.to_uppercase(), 17), 72.0, 78, 15, WHITE),
		text_path(badge, 72.0, 108, 12, accent),
		text_path("PRESS TO RUN", 72.0, 129, 7, "#586576"),
	);
	shell(accent, &content)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::model::{ActionKind, CurrencyData, WidgetData};
	use serde_json::json;

	#[test]
	fn monetary_values_use_grouping_and_exactly_two_decimals() {
		assert_eq!(number_2(1_000.0), "1,000.00");
		assert_eq!(number_2(35.1234), "35.12");
		assert_eq!(number_2(-12_345.678), "-12,345.68");
		assert_eq!(signed_number_2(9_876.5), "+9,876.50");
		assert_eq!(number_2(-0.001), "0.00");
	}

	#[test]
	fn generated_images_are_small_encoded_svg_frames() {
		let image = widget(
			ActionKind::Currency,
			&WidgetData::Currency(CurrencyData {
				pair: "USDTHB".to_owned(),
				price: 35.1234,
				change_percent: 0.25,
			}),
			&json!({}),
		);
		assert!(image.starts_with("data:image/svg+xml,"));
		assert!(image.contains("width%3D%22144%22"));
		assert!(image.contains("fill%3D%22%23000%22"));
		assert!(!image.contains("%230b0f15"));
		assert!(!image.contains("M24%2018v112"));
		assert!(!image.contains("%3Ctext"));
		assert!(image.len() < 50_000);
	}
}
