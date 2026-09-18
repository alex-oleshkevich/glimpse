use std::sync::Arc;

use tokio::sync::{Mutex, watch};

use super::Registry;

/// The inhibitor registry, shared across every D-Bus surface that mutates or reads it, plus two
/// `watch` channels: `any_idle_target` for the idle actor's gate, `generation` for the crate's one
/// `Inhibitors`-changed emitter. See the crate README for what each tracks and why.
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
mod tests {
    use super::*;
    use glimpse_dbus::idle::{
        IdleInhibitorRecord, IdleInhibitorSource, InhibitionTargets, Login1Mode,
    };

    fn idle_record(id: u64) -> IdleInhibitorRecord {
        IdleInhibitorRecord {
            id,
            who: "who".into(),
            why: "why".into(),
            bus_name: String::new(),
            process_name: String::new(),
            source: IdleInhibitorSource::login1(0, 0, Login1Mode::Block),
            targets: InhibitionTargets::idle_only(),
            can_release: true,
            added_at_unix: 0,
        }
    }

    #[tokio::test]
    async fn mutate_republishes_any_idle_target_only_when_it_changes() {
        let (shared, mut receiver, _generation) = SharedRegistry::new();
        assert!(!*receiver.borrow_and_update());

        let id = shared.mutate(|registry| registry.mint_id()).await;
        shared
            .mutate(|registry| registry.insert(idle_record(id), None))
            .await;
        assert!(receiver.has_changed().unwrap());
        assert!(*receiver.borrow_and_update());

        shared.mutate(|registry| registry.mint_id()).await;
        assert!(
            !receiver.has_changed().unwrap(),
            "a mutation that leaves any_idle_target unchanged must not wake the subscriber"
        );

        shared.mutate(|registry| registry.release_record(id)).await;
        assert!(receiver.has_changed().unwrap());
        assert!(!*receiver.borrow_and_update());
    }

    #[tokio::test]
    async fn read_sees_what_mutate_wrote() {
        let (shared, _receiver, _generation) = SharedRegistry::new();
        let id = shared.mutate(|registry| registry.mint_id()).await;
        shared
            .mutate(|registry| registry.insert(idle_record(id), None))
            .await;

        let snapshot = shared.read(|registry| registry.snapshot()).await;
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].id, id);
    }

    #[tokio::test]
    async fn generation_tracks_registry_version_and_skips_state_preserving_mutations() {
        let (shared, _idle_target, mut generation) = SharedRegistry::new();
        assert_eq!(*generation.borrow_and_update(), 0);

        // Minting alone changes nothing a reader could observe, so it must not wake a subscriber
        // — this is the case that used to defeat the rate limiter by broadcasting on every
        // rejected Inhibit.
        shared.mutate(|registry| registry.mint_id()).await;
        assert!(
            !generation.has_changed().unwrap(),
            "minting alone must not bump generation"
        );

        let id = shared.mutate(|registry| registry.mint_id()).await;
        shared
            .mutate(|registry| registry.insert(idle_record(id), None))
            .await;
        assert!(generation.has_changed().unwrap());
        assert_eq!(*generation.borrow_and_update(), 1);

        shared
            .mutate(|registry| registry.set_process_name(id, "firefox".to_owned()))
            .await;
        assert!(generation.has_changed().unwrap());
        assert_eq!(*generation.borrow_and_update(), 2);

        shared.mutate(|registry| registry.release_record(id)).await;
        assert!(generation.has_changed().unwrap());
        assert_eq!(*generation.borrow_and_update(), 3);

        // Releasing an id already gone (an unknown UnInhibit cookie, a disconnect from a bus name
        // holding nothing) must not broadcast either.
        shared.mutate(|registry| registry.release_record(id)).await;
        assert!(!generation.has_changed().unwrap());
    }
}
