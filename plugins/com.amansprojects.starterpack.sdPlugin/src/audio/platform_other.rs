use super::AudioSnapshot;
use anyhow::{Result, bail};

fn unsupported<T>() -> Result<T> {
	bail!("system audio actions are currently available on Windows")
}

pub fn snapshot() -> Result<AudioSnapshot> {
	unsupported()
}

pub fn change_volume(_delta: i32) -> Result<AudioSnapshot> {
	unsupported()
}

pub fn switch_device(_direction: i32) -> Result<AudioSnapshot> {
	unsupported()
}
