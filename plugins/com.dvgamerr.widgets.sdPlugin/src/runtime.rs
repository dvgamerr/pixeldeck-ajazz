use openaction::{Instance, OpenActionResult, get_instance};
use serde_json::Value as SettingsValue;
use std::{
	collections::HashMap,
	sync::{LazyLock, Mutex},
	time::{Duration, Instant},
};
use tokio::task::JoinHandle;

use crate::{
	fetch,
	model::{ActionKind, WidgetData, setting_f64, setting_string},
	render,
};

const MAX_CACHE_ENTRIES: usize = 128;
type CacheKey = (ActionKind, String);

struct Entry {
	kind: ActionKind,
	settings: SettingsValue,
	generation: u64,
	next_due: Instant,
	busy: bool,
	data: Option<WidgetData>,
	last_image: Option<String>,
}

struct CacheEntry {
	data: WidgetData,
	last_used: u64,
}

#[derive(Default)]
struct Runtime {
	entries: HashMap<String, Entry>,
	cache: HashMap<CacheKey, CacheEntry>,
	cache_clock: u64,
	worker: Option<JoinHandle<()>>,
}

struct JobGroup {
	kind: ActionKind,
	settings: SettingsValue,
	targets: Vec<(String, u64)>,
}

static RUNTIME: LazyLock<Mutex<Runtime>> = LazyLock::new(Default::default);

fn cache_key(kind: ActionKind, settings: &SettingsValue) -> CacheKey {
	(kind, request_key(kind, settings))
}

fn cached_data(runtime: &mut Runtime, key: &CacheKey) -> Option<WidgetData> {
	runtime.cache_clock = runtime.cache_clock.wrapping_add(1);
	let last_used = runtime.cache_clock;
	runtime.cache.get_mut(key).map(|entry| {
		entry.last_used = last_used;
		entry.data.clone()
	})
}

fn cache_data(runtime: &mut Runtime, key: CacheKey, data: WidgetData) {
	runtime.cache_clock = runtime.cache_clock.wrapping_add(1);
	if !runtime.cache.contains_key(&key)
		&& runtime.cache.len() >= MAX_CACHE_ENTRIES
		&& let Some(oldest) = runtime
			.cache
			.iter()
			.min_by_key(|(_, entry)| entry.last_used)
			.map(|(key, _)| key.clone())
	{
		runtime.cache.remove(&oldest);
	}
	runtime.cache.insert(
		key,
		CacheEntry {
			data,
			last_used: runtime.cache_clock,
		},
	);
}

fn initial_frame(
	runtime: &mut Runtime,
	kind: ActionKind,
	settings: &SettingsValue,
) -> (Option<WidgetData>, String) {
	let data = cached_data(runtime, &cache_key(kind, settings));
	let image = data.as_ref().map_or_else(
		|| render::loading(kind),
		|data| render::widget(kind, data, settings),
	);
	(data, image)
}

fn ensure_worker(runtime: &mut Runtime) {
	if runtime
		.worker
		.as_ref()
		.is_some_and(|worker| !worker.is_finished())
	{
		return;
	}
	runtime.worker = Some(tokio::spawn(scheduler()));
}

pub async fn appear(
	context: String,
	kind: ActionKind,
	settings: SettingsValue,
	instance: &Instance,
) -> OpenActionResult<()> {
	let image = {
		let mut runtime = RUNTIME.lock().unwrap();
		let (data, image) = initial_frame(&mut runtime, kind, &settings);
		runtime.entries.insert(
			context.clone(),
			Entry {
				kind,
				settings,
				generation: 0,
				next_due: Instant::now(),
				busy: false,
				data,
				last_image: Some(image.clone()),
			},
		);
		ensure_worker(&mut runtime);
		image
	};
	instance.set_image(Some(image), None).await?;
	Ok(())
}

pub fn disappear(context: &str) {
	let mut runtime = RUNTIME.lock().unwrap();
	runtime.entries.remove(context);
	if runtime.entries.is_empty()
		&& let Some(worker) = runtime.worker.take()
	{
		worker.abort();
	}
}

pub fn update(context: &str, settings: SettingsValue) -> Option<String> {
	let mut runtime = RUNTIME.lock().unwrap();
	let kind = runtime.entries.get(context)?.kind;
	let (data, image) = initial_frame(&mut runtime, kind, &settings);
	let entry = runtime.entries.get_mut(context)?;
	entry.settings = settings;
	entry.generation = entry.generation.wrapping_add(1);
	entry.next_due = Instant::now();
	entry.data = data;
	entry.last_image = Some(image.clone());
	Some(image)
}

pub fn refresh(context: &str) {
	if let Some(entry) = RUNTIME.lock().unwrap().entries.get_mut(context) {
		entry.next_due = Instant::now();
	}
}

fn collect_jobs() -> Vec<JobGroup> {
	let now = Instant::now();
	let mut runtime = RUNTIME.lock().unwrap();
	let mut groups: HashMap<(ActionKind, String), JobGroup> = HashMap::new();
	for (context, entry) in &mut runtime.entries {
		if entry.busy || entry.next_due > now {
			continue;
		}
		entry.busy = true;
		entry.next_due = now + entry.kind.refresh_interval(&entry.settings);
		let key = (entry.kind, request_key(entry.kind, &entry.settings));
		groups
			.entry(key)
			.or_insert_with(|| JobGroup {
				kind: entry.kind,
				settings: entry.settings.clone(),
				targets: Vec::new(),
			})
			.targets
			.push((context.clone(), entry.generation));
	}
	groups.into_values().collect()
}

fn request_key(kind: ActionKind, settings: &SettingsValue) -> String {
	match kind {
		ActionKind::Gold => "gold".to_owned(),
		ActionKind::Currency => format!(
			"{}:{}",
			setting_string(settings, "from", "USD").to_uppercase(),
			setting_string(settings, "to", "THB").to_uppercase()
		),
		ActionKind::AirQuality => setting_string(
			settings,
			"url",
			"https://www.iqair.com/th-en/thailand/bangkok/nong-khaem",
		),
		ActionKind::Weather => format!(
			"{}:{}",
			setting_string(settings, "lat", "13.72"),
			setting_string(settings, "lon", "100.41")
		),
		ActionKind::WorkHours => "work-hours".to_owned(),
		ActionKind::Stock => format!(
			"{}:{}:{}:{}",
			setting_string(settings, "symbol", "AAPL").to_uppercase(),
			setting_string(settings, "displayName", ""),
			setting_f64(settings, "cost").map_or_else(String::new, |value| value.to_string()),
			setting_f64(settings, "qty").map_or_else(String::new, |value| value.to_string()),
		),
		_ => settings.to_string(),
	}
}

async fn scheduler() {
	let mut interval = tokio::time::interval(Duration::from_secs(1));
	interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
	loop {
		interval.tick().await;
		for group in collect_jobs() {
			tokio::spawn(async move {
				let result = fetch::widget(group.kind, &group.settings)
					.await
					.map_err(|error| format!("{error:#}"));
				for (context, generation) in group.targets {
					finish(context, generation, result.clone()).await;
				}
			});
		}
	}
}

async fn finish(context: String, generation: u64, result: Result<WidgetData, String>) {
	let image = {
		let mut runtime = RUNTIME.lock().unwrap();
		let mut cache_update = None;
		let image = {
			let Some(entry) = runtime.entries.get_mut(&context) else {
				return;
			};
			entry.busy = false;
			if entry.generation != generation {
				entry.next_due = Instant::now();
				return;
			}
			let image = match result {
				Ok(data) => {
					let image = render::widget(entry.kind, &data, &entry.settings);
					cache_update = Some((cache_key(entry.kind, &entry.settings), data.clone()));
					entry.data = Some(data);
					Some(image)
				}
				Err(error) => {
					log::warn!("{:?} refresh failed for {context}: {error}", entry.kind);
					entry.data.is_none().then(|| render::error(&error))
				}
			};
			image.and_then(|image| {
				if entry.last_image.as_ref() == Some(&image) {
					None
				} else {
					entry.last_image = Some(image.clone());
					Some(image)
				}
			})
		};
		if let Some((key, data)) = cache_update {
			cache_data(&mut runtime, key, data);
		}
		image
	};
	let Some(image) = image else {
		return;
	};

	if let Some(instance) = get_instance(context).await
		&& let Err(error) = instance.set_image(Some(image), None).await
	{
		log::warn!("widget image update failed: {error}");
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::model::CurrencyData;
	use serde_json::json;

	#[test]
	fn cached_frame_replaces_loading_after_context_disappears() {
		let settings = json!({ "from": "USD", "to": "THB", "interval": 10_000 });
		let mut runtime = Runtime::default();
		let (data, first_image) = initial_frame(&mut runtime, ActionKind::Currency, &settings);
		assert!(data.is_none());
		assert_eq!(first_image, render::loading(ActionKind::Currency));

		let cached = WidgetData::Currency(CurrencyData {
			pair: "USDTHB".to_owned(),
			price: 32.5,
			change_percent: 0.25,
		});
		cache_data(
			&mut runtime,
			cache_key(ActionKind::Currency, &settings),
			cached.clone(),
		);
		let (data, cached_image) = initial_frame(&mut runtime, ActionKind::Currency, &settings);
		assert!(data.is_some());
		assert_eq!(
			cached_image,
			render::widget(ActionKind::Currency, &cached, &settings)
		);
		assert_ne!(cached_image, first_image);
	}

	#[test]
	fn identical_requests_share_one_scheduler_group() {
		let mut runtime = RUNTIME.lock().unwrap();
		runtime.entries.clear();
		for (context, interval) in [("one", 5_000), ("two", 60_000)] {
			runtime.entries.insert(
				context.to_owned(),
				Entry {
					kind: ActionKind::Currency,
					settings: json!({ "from": "USD", "to": "THB", "interval": interval }),
					generation: 0,
					next_due: Instant::now(),
					busy: false,
					data: None,
					last_image: None,
				},
			);
		}
		drop(runtime);
		let jobs = collect_jobs();
		assert_eq!(jobs.len(), 1);
		assert_eq!(jobs[0].targets.len(), 2);
		RUNTIME.lock().unwrap().entries.clear();
	}
}
