use super::MonitorSnapshot;

pub struct Sampler;

impl Sampler {
	pub fn new() -> Self {
		Self
	}

	pub fn snapshot(&mut self) -> MonitorSnapshot {
		MonitorSnapshot {
			cpu: 0,
			gpu: None,
			cpu_temperature: None,
			gpu_temperature: None,
			memory: 0,
			memory_total_mib: 0,
			memory_available_mib: 0,
			pagefile_total_mib: 0,
			pagefile_available_mib: 0,
		}
	}
}
