use serde_json::Value as SettingsValue;
use std::time::Duration;

#[cfg(test)]
pub const PREFIX: &str = "com.dvgamerr.widgets.";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActionKind {
	Gold,
	Currency,
	Stock,
	AirQuality,
	PowerShell,
	Weather,
	WorkHours,
}

impl ActionKind {
	#[cfg(test)]
	pub fn from_uuid(uuid: &str) -> Option<Self> {
		match uuid.strip_prefix(PREFIX)? {
			"gold-price" => Some(Self::Gold),
			"currency-rate" => Some(Self::Currency),
			"stock-price" => Some(Self::Stock),
			"air-quality" => Some(Self::AirQuality),
			"powershell" => Some(Self::PowerShell),
			"weather" => Some(Self::Weather),
			"work-hours" => Some(Self::WorkHours),
			_ => None,
		}
	}

	pub fn scheduled(self) -> bool {
		matches!(
			self,
			Self::Gold
				| Self::Currency
				| Self::Stock
				| Self::AirQuality
				| Self::Weather
				| Self::WorkHours
		)
	}

	pub fn refresh_interval(self, settings: &SettingsValue) -> Duration {
		let milliseconds = match self {
			Self::Gold => setting_u64(settings, "interval", 15).clamp(3, 3_600) * 1_000,
			Self::Currency | Self::Stock => {
				setting_u64(settings, "interval", 10_000).clamp(5_000, 3_600_000)
			}
			Self::AirQuality => {
				setting_u64(settings, "interval", 3_600_000).clamp(60_000, 86_400_000)
			}
			Self::Weather => {
				setting_u64(settings, "interval", 1_800_000).clamp(300_000, 86_400_000)
			}
			Self::WorkHours => 60_000,
			_ => 86_400_000,
		};
		Duration::from_millis(milliseconds)
	}
}

#[derive(Clone, Debug)]
pub struct GoldData {
	pub price_usd: f64,
	pub exchange_rate: f64,
}

#[derive(Clone, Debug)]
pub struct CurrencyData {
	pub pair: String,
	pub price: f64,
	pub change_percent: f64,
}

#[derive(Clone, Debug)]
pub struct StockData {
	pub symbol: String,
	pub display_name: String,
	pub price: f64,
	pub open: Option<f64>,
	pub change_percent: f64,
	pub profit_loss: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct AirQualityData {
	pub aqi: u16,
	pub level: String,
}

#[derive(Clone, Debug)]
pub struct WeatherData {
	pub temperature: i16,
	pub apparent: i16,
	pub code: u8,
	pub is_day: bool,
	pub precipitation: u8,
}

#[derive(Clone, Debug)]
pub enum WidgetData {
	Gold(GoldData),
	Currency(CurrencyData),
	Stock(StockData),
	AirQuality(AirQualityData),
	Weather(WeatherData),
	WorkHours,
}

pub fn setting_string(settings: &SettingsValue, key: &str, fallback: &str) -> String {
	settings
		.as_object()
		.and_then(|settings| settings.get(key))
		.and_then(SettingsValue::as_str)
		.map(str::trim)
		.filter(|value| !value.is_empty())
		.unwrap_or(fallback)
		.to_owned()
}

pub fn setting_f64(settings: &SettingsValue, key: &str) -> Option<f64> {
	settings
		.as_object()
		.and_then(|settings| settings.get(key))
		.and_then(|value| {
			value
				.as_f64()
				.or_else(|| value.as_str()?.trim().parse().ok())
		})
		.filter(|value| value.is_finite())
}

pub fn setting_i64(settings: &SettingsValue, key: &str, fallback: i64) -> i64 {
	settings
		.as_object()
		.and_then(|settings| settings.get(key))
		.and_then(|value| {
			value
				.as_i64()
				.or_else(|| value.as_str()?.trim().parse().ok())
		})
		.unwrap_or(fallback)
}

pub fn setting_u64(settings: &SettingsValue, key: &str, fallback: u64) -> u64 {
	settings
		.as_object()
		.and_then(|settings| settings.get(key))
		.and_then(|value| {
			value
				.as_u64()
				.or_else(|| value.as_str()?.trim().parse().ok())
		})
		.unwrap_or(fallback)
}
