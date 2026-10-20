mod control;
mod health;
pub(crate) mod login1_observer;
pub(crate) mod portal;
mod registry;
pub(crate) mod screen_saver;
mod shared;
#[cfg(test)]
pub(crate) mod test_support;

pub use control::Idle1Server;
pub use health::{Backend, Health};
pub use registry::Registry;
pub use shared::SharedRegistry;

pub(crate) const WHO_CAP: usize = 120;
pub(crate) const WHY_CAP: usize = 240;

pub(crate) fn unix_now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
