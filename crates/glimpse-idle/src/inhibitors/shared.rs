use std::sync::Arc;

use tokio::sync::{Mutex, watch};

use super::Registry;

pub struct SharedRegistry {
    registry: Mutex<Registry>,
    any_idle_target: watch::Sender<bool>,
    generation: watch::Sender<u64>,
}

impl SharedRegistry {
    pub fn new() -> (Arc<Self>, watch::Receiver<bool>, watch::Receiver<u64>) {
        let (any_idle_target, idle_target_receiver) = watch::channel(false);
        let (generation, generation_receiver) = watch::channel(0u64);
        (
            Arc::new(Self {
                registry: Mutex::new(Registry::default()),
                any_idle_target,
                generation,
            }),
            idle_target_receiver,
            generation_receiver,
        )
    }

    pub async fn mutate<T>(&self, f: impl FnOnce(&mut Registry) -> T) -> T {
        let mut registry = self.registry.lock().await;
        let result = f(&mut registry);
        let any_idle_target = registry.any_idle_target();
        let version = registry.version();
        drop(registry);
        self.any_idle_target.send_if_modified(|current| {
            if *current == any_idle_target {
                false
            } else {
                *current = any_idle_target;
                true
            }
        });
        self.generation.send_if_modified(|current| {
            if *current == version {
                false
            } else {
                *current = version;
                true
            }
        });
        result
    }

    pub async fn read<T>(&self, f: impl FnOnce(&Registry) -> T) -> T {
        let registry = self.registry.lock().await;
        f(&registry)
    }
}

#[cfg(test)]
mod tests;
