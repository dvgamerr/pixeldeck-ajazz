use crate::compat::{EventHandlerResult, OUTBOUND_EVENT_MANAGER};
use std::{
	collections::HashSet,
	sync::{
		Arc, Mutex,
		atomic::{AtomicBool, Ordering},
	},
};
use tokio::task::JoinHandle;

#[derive(Default)]
pub struct LiveAction {
	contexts: HashSet<String>,
	worker: Option<(JoinHandle<()>, Arc<AtomicBool>)>,
}

impl LiveAction {
	pub fn subscribe(&mut self, context: String) -> bool {
		self.contexts.insert(context);
		self.worker
			.as_ref()
			.is_none_or(|(worker, _)| worker.is_finished())
	}

	pub fn start(&mut self, worker: JoinHandle<()>, cancel: Arc<AtomicBool>) {
		self.stop_worker();
		self.worker = Some((worker, cancel));
	}

	pub fn unsubscribe(&mut self, context: &str) {
		self.contexts.remove(context);
		if self.contexts.is_empty() {
			self.stop_worker();
		}
	}

	fn stop_worker(&mut self) {
		if let Some((worker, cancel)) = self.worker.take() {
			cancel.store(true, Ordering::Release);
			worker.abort();
		}
	}
}

impl Drop for LiveAction {
	fn drop(&mut self) {
		self.stop_worker();
	}
}

pub async fn broadcast(live: &'static Mutex<LiveAction>, image: String) -> EventHandlerResult {
	broadcast_mapped(live, |_| image.clone()).await
}

pub async fn broadcast_mapped(
	live: &'static Mutex<LiveAction>,
	mut image_for: impl FnMut(&str) -> String,
) -> EventHandlerResult {
	let contexts: Vec<_> = live.lock().unwrap().contexts.iter().cloned().collect();
	if contexts.is_empty() {
		return Ok(());
	}

	let mut manager = OUTBOUND_EVENT_MANAGER.lock().await;
	if let Some(outbound) = manager.as_mut() {
		for context in contexts {
			outbound
				.set_image(context.clone(), Some(image_for(&context)), None)
				.await?;
		}
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::LiveAction;
	use std::sync::{
		Arc,
		atomic::{AtomicBool, Ordering},
	};

	#[tokio::test]
	async fn contexts_share_one_worker_until_the_last_unsubscribes() {
		let mut live = LiveAction::default();
		assert!(live.subscribe("first".to_owned()));

		let cancel = Arc::new(AtomicBool::new(false));
		let worker = tokio::spawn(std::future::pending::<()>());
		live.start(worker, cancel.clone());

		assert!(!live.subscribe("second".to_owned()));
		live.unsubscribe("first");
		assert!(!cancel.load(Ordering::Acquire));

		live.unsubscribe("second");
		assert!(cancel.load(Ordering::Acquire));
	}
}
