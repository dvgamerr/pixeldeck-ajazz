use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;
use reqwest::{Client, Response, Url};
use serde_json::Value as SettingsValue;
use serde_json::Value;
use std::{sync::LazyLock, time::Duration};

use crate::model::{
	ActionKind, AirQualityData, CurrencyData, GoldData, StockData, WeatherData, WidgetData,
	setting_f64, setting_string,
};

const DEFAULT_AQI_URL: &str = "https://www.iqair.com/th-en/thailand/bangkok/nong-khaem";

static CLIENT: LazyLock<Client> = LazyLock::new(|| {
	Client::builder()
		.user_agent(
			"Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
			 Chrome/138 Safari/537.36 PixelDeck-Widgets/1.0",
		)
		.timeout(Duration::from_secs(15))
		.connect_timeout(Duration::from_secs(7))
		.pool_idle_timeout(Duration::from_secs(90))
		.build()
		.expect("failed to build HTTP client")
});
static AQI_STRICT: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(
		r#"(?is)class=[\"']([^\"']*aqi-legend-bg-[^\"']*)[\"'][^>]*>.*?<p[^>]*>\s*([0-9]{1,3})\s*</p>.*?US AQI.*?</div>\s*<p[^>]*class=[\"'][^\"']*font-body-l-medium[^\"']*[\"'][^>]*>\s*([^<]+?)\s*</p>"#,
	)
	.unwrap()
});
static AQI_VALUE: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r#">\s*([0-9]{1,3})\s*<"#).unwrap());

/// A refresh that produced no data.
#[derive(Clone)]
pub struct Failure {
	pub message: String,
	/// The source was never reached — the machine is offline or the upstream API is
	/// down — rather than the widget being misconfigured. Those refreshes are skipped
	/// silently so a dropped connection never repaints a working widget.
	pub unreachable: bool,
}

impl From<anyhow::Error> for Failure {
	fn from(error: anyhow::Error) -> Self {
		Self {
			// Every reqwest failure is a transport or HTTP-status problem; bad settings
			// surface as our own `bail!`/`anyhow!` messages instead.
			unreachable: error.chain().any(|cause| cause.is::<reqwest::Error>()),
			message: format!("{error:#}"),
		}
	}
}

pub async fn widget(kind: ActionKind, settings: &SettingsValue) -> Result<WidgetData> {
	match kind {
		ActionKind::Gold => fetch_gold().await.map(WidgetData::Gold),
		ActionKind::Currency => fetch_currency(settings).await.map(WidgetData::Currency),
		ActionKind::Stock => fetch_stock(settings).await.map(WidgetData::Stock),
		ActionKind::AirQuality => fetch_air_quality(settings)
			.await
			.map(WidgetData::AirQuality),
		ActionKind::Weather => fetch_weather(settings).await.map(WidgetData::Weather),
		ActionKind::WorkHours => Ok(WidgetData::WorkHours),
		_ => bail!("action has no scheduled data source"),
	}
}

fn number(value: Option<&Value>) -> Option<f64> {
	value
		.and_then(|value| {
			value
				.as_f64()
				.or_else(|| value.as_str()?.trim().parse().ok())
		})
		.filter(|value| value.is_finite())
}

async fn fetch_gold() -> Result<GoldData> {
	let payload: Value = CLIENT
		.get("https://register.ylgbullion.co.th/api/price/gold")
		.send()
		.await
		.context("YLG request failed")?
		.error_for_status()
		.context("YLG returned an error")?
		.json()
		.await
		.context("invalid YLG response")?;
	let price_usd =
		number(payload.pointer("/spot/tin")).ok_or_else(|| anyhow!("YLG spot price is missing"))?;
	let exchange_rate = number(payload.get("exchange_sale")).unwrap_or(1.0);
	Ok(GoldData {
		price_usd,
		exchange_rate,
	})
}

fn yahoo_price(result: &Value) -> Option<f64> {
	number(result.pointer("/meta/regularMarketPrice")).or_else(|| {
		result
			.pointer("/indicators/quote/0/close")?
			.as_array()?
			.iter()
			.rev()
			.find_map(|value| number(Some(value)))
	})
}

async fn yahoo_chart(symbol: &str, interval: &str, range: &str) -> Result<Value> {
	let symbol = urlencoding::encode(symbol);
	let mut last_error = None;
	let mut payload = None;
	for host in ["query1.finance.yahoo.com", "query2.finance.yahoo.com"] {
		let url =
			format!("https://{host}/v8/finance/chart/{symbol}?interval={interval}&range={range}");
		match CLIENT
			.get(url)
			.send()
			.await
			.and_then(Response::error_for_status)
		{
			Ok(response) => {
				payload = Some(
					response
						.json::<Value>()
						.await
						.context("invalid Yahoo Finance response")?,
				);
				break;
			}
			Err(error) => {
				last_error =
					Some(anyhow::Error::from(error).context("Yahoo Finance request failed"))
			}
		}
	}
	let payload = payload
		.ok_or_else(|| last_error.unwrap_or_else(|| anyhow!("Yahoo Finance request failed")))?;
	payload
		.pointer("/chart/result/0")
		.cloned()
		.ok_or_else(|| anyhow!("Yahoo Finance returned no data"))
}

async fn fetch_currency(settings: &SettingsValue) -> Result<CurrencyData> {
	let from = setting_string(settings, "from", "USD").to_uppercase();
	let to = setting_string(settings, "to", "THB").to_uppercase();
	if !from
		.chars()
		.all(|character| character.is_ascii_alphabetic())
		|| !to.chars().all(|character| character.is_ascii_alphabetic())
	{
		bail!("invalid currency pair");
	}
	let pair = format!("{from}{to}");
	let result = yahoo_chart(&format!("{pair}=X"), "1d", "1d").await?;
	let price = yahoo_price(&result).ok_or_else(|| anyhow!("currency rate is missing"))?;
	let previous = number(result.pointer("/meta/previousClose"))
		.or_else(|| number(result.pointer("/meta/chartPreviousClose")))
		.unwrap_or(price);
	let change_percent = if previous.abs() > f64::EPSILON {
		(price - previous) / previous * 100.0
	} else {
		0.0
	};
	Ok(CurrencyData {
		pair,
		price,
		change_percent,
	})
}

async fn fetch_stock(settings: &SettingsValue) -> Result<StockData> {
	let symbol = setting_string(settings, "symbol", "AAPL").to_uppercase();
	if symbol.len() > 24
		|| !symbol
			.chars()
			.all(|character| character.is_ascii_alphanumeric() || ".^-=".contains(character))
	{
		bail!("invalid stock symbol");
	}
	let result = yahoo_chart(&symbol, "1mo", "1mo").await?;
	let price = yahoo_price(&result).ok_or_else(|| anyhow!("stock price is missing"))?;
	let open = number(result.pointer("/meta/regularMarketOpen")).or_else(|| {
		result
			.pointer("/indicators/quote/0/open")?
			.as_array()?
			.iter()
			.find_map(|value| number(Some(value)))
	});
	let previous = number(result.pointer("/meta/previousClose"))
		.or_else(|| number(result.pointer("/meta/chartPreviousClose")))
		.unwrap_or(price);
	let cost = setting_f64(settings, "cost").filter(|value| *value > 0.0);
	let quantity = setting_f64(settings, "qty").filter(|value| *value > 0.0);
	let change_percent = cost
		.map(|cost| (price - cost) / cost * 100.0)
		.unwrap_or_else(|| {
			if previous.abs() > f64::EPSILON {
				(price - previous) / previous * 100.0
			} else {
				0.0
			}
		});
	let profit_loss = cost
		.zip(quantity)
		.map(|(cost, quantity)| (price - cost) * quantity);
	Ok(StockData {
		symbol,
		display_name: setting_string(settings, "displayName", ""),
		price,
		open,
		change_percent,
		profit_loss,
	})
}

fn iqair_url(settings: &SettingsValue) -> Result<Url> {
	let value = setting_string(settings, "url", DEFAULT_AQI_URL);
	let url = Url::parse(&value).or_else(|_| Url::parse(&format!("https://{value}")))?;
	let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
	if url.scheme() != "https" || !(host == "iqair.com" || host.ends_with(".iqair.com")) {
		bail!("only HTTPS IQAir location URLs are allowed");
	}
	Ok(url)
}

fn decode_html(value: &str) -> String {
	value
		.replace("&amp;", "&")
		.replace("&#39;", "'")
		.replace("&quot;", "\"")
		.replace("&nbsp;", " ")
}

fn parse_air_quality(html: &str) -> Result<AirQualityData> {
	if let Some(captures) = AQI_STRICT.captures(html) {
		return Ok(AirQualityData {
			aqi: captures[2].parse()?,
			level: decode_html(captures[3].trim()),
		});
	}

	let label = html
		.to_ascii_lowercase()
		.find("us aqi")
		.ok_or_else(|| anyhow!("US AQI label not found"))?;
	let before = &html[label.saturating_sub(4_000)..label];
	let aqi = AQI_VALUE
		.captures_iter(before)
		.last()
		.and_then(|captures| captures[1].parse().ok())
		.ok_or_else(|| anyhow!("AQI value not found"))?;
	Ok(AirQualityData {
		aqi,
		level: String::new(),
	})
}

async fn fetch_air_quality(settings: &SettingsValue) -> Result<AirQualityData> {
	let url = iqair_url(settings)?;
	let iqair_result = async {
		let response = CLIENT
			.get(url.clone())
			.send()
			.await
			.context("IQAir request failed")?
			.error_for_status()
			.context("IQAir returned an error")?;
		parse_air_quality(&response.text().await.context("invalid IQAir response")?)
	}
	.await;
	match iqair_result {
		Ok(data) => Ok(data),
		Err(error) => {
			log::debug!("IQAir scrape failed; using Open-Meteo AQI fallback: {error:#}");
			fetch_open_meteo_aqi(&url).await
		}
	}
}

async fn fetch_open_meteo_aqi(iqair_url: &Url) -> Result<AirQualityData> {
	let location = iqair_url
		.path_segments()
		.and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
		.unwrap_or("Bangkok")
		.replace('-', " ");
	let geocoding: Value = CLIENT
		.get("https://geocoding-api.open-meteo.com/v1/search")
		.query(&[
			("name", location),
			("count", "1".to_owned()),
			("language", "en".to_owned()),
			("format", "json".to_owned()),
		])
		.send()
		.await
		.context("AQI location lookup failed")?
		.error_for_status()
		.context("AQI location lookup returned an error")?
		.json()
		.await
		.context("invalid AQI location response")?;
	let latitude = number(geocoding.pointer("/results/0/latitude"))
		.ok_or_else(|| anyhow!("AQI location was not found"))?;
	let longitude = number(geocoding.pointer("/results/0/longitude"))
		.ok_or_else(|| anyhow!("AQI location was not found"))?;
	let payload: Value = CLIENT
		.get("https://air-quality-api.open-meteo.com/v1/air-quality")
		.query(&[
			("latitude", latitude.to_string()),
			("longitude", longitude.to_string()),
			("current", "us_aqi".to_owned()),
			("timezone", "auto".to_owned()),
		])
		.send()
		.await
		.context("Open-Meteo AQI request failed")?
		.error_for_status()
		.context("Open-Meteo AQI returned an error")?
		.json()
		.await
		.context("invalid Open-Meteo AQI response")?;
	let aqi = number(payload.pointer("/current/us_aqi"))
		.ok_or_else(|| anyhow!("US AQI value is missing"))?
		.round()
		.clamp(0.0, u16::MAX as f64) as u16;
	Ok(AirQualityData {
		aqi,
		level: String::new(),
	})
}

async fn fetch_weather(settings: &SettingsValue) -> Result<WeatherData> {
	let latitude = setting_f64(settings, "lat")
		.unwrap_or(13.72)
		.clamp(-90.0, 90.0);
	let longitude = setting_f64(settings, "lon")
		.unwrap_or(100.41)
		.clamp(-180.0, 180.0);
	let payload: Value = CLIENT
		.get("https://api.open-meteo.com/v1/forecast")
		.query(&[
			("latitude", latitude.to_string()),
			("longitude", longitude.to_string()),
			(
				"current",
				"temperature_2m,apparent_temperature,weather_code,is_day,precipitation_probability"
					.to_owned(),
			),
			("timezone", "auto".to_owned()),
			("forecast_days", "1".to_owned()),
		])
		.send()
		.await
		.context("Open-Meteo request failed")?
		.error_for_status()
		.context("Open-Meteo returned an error")?
		.json()
		.await
		.context("invalid Open-Meteo response")?;
	let current = payload
		.get("current")
		.ok_or_else(|| anyhow!("weather data is missing"))?;
	Ok(WeatherData {
		temperature: number(current.get("temperature_2m"))
			.ok_or_else(|| anyhow!("temperature is missing"))?
			.round() as i16,
		apparent: number(current.get("apparent_temperature"))
			.unwrap_or_default()
			.round() as i16,
		code: number(current.get("weather_code")).unwrap_or_default() as u8,
		is_day: number(current.get("is_day")).unwrap_or(1.0) == 1.0,
		precipitation: number(current.get("precipitation_probability"))
			.unwrap_or_default()
			.clamp(0.0, 100.0) as u8,
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn transport_failures_are_skippable_but_bad_settings_are_not() {
		let refused = CLIENT
			.get("http://127.0.0.1:1/")
			.send()
			.await
			.expect_err("a closed port must not answer");
		let refused = Failure::from(anyhow::Error::from(refused).context("request failed"));
		assert!(refused.unreachable);

		let rejected = fetch_currency(&serde_json::json!({ "from": "US1", "to": "THB" }))
			.await
			.expect_err("a non-alphabetic currency must be rejected before any request");
		assert!(!Failure::from(rejected).unreachable);
	}

	#[test]
	fn parses_the_iqair_badge_shape_used_by_the_source_widget() {
		let html = r#"<div class="x aqi-legend-bg-green"><p> 42 </p><span>US AQI</span></div><p class="font-body-l-medium">Good</p>"#;
		let parsed = parse_air_quality(html).unwrap();
		assert_eq!(parsed.aqi, 42);
		assert_eq!(parsed.level, "Good");
	}
}
