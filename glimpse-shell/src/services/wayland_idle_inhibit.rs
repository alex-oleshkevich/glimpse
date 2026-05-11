use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use glimpse_core::services::idle_inhibitor::State;

/// Health of the shell-side Wayland idle-inhibit backend. Separate from
/// the daemon-side InhibitorsHealth — the daemon doesn't know whether
/// the shell can actually attach a zwp_idle_inhibitor_v1 to a visible
/// surface (which is what the compositor needs to honor inhibition).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaylandHealth {
    Ready,
    Unsupported { message: String },
}

/// Drives a single zwp_idle_inhibitor_v1. Implementations must be safe to
/// call set_inhibited(true) when already inhibited (idempotent) and the
/// same for false.
///
/// The real GDK-backed binding (GDK display → zwp_idle_inhibit_manager_v1 →
/// attach to panel wl_surface) is intentionally out of scope for this task
/// and will land in a follow-up. The default backend today is
/// NoopWaylandInhibitor, which logs toggles and reports Unsupported health.
pub trait WaylandIdleInhibitor: Send {
    fn set_inhibited(&mut self, inhibited: bool);
    fn health(&self) -> WaylandHealth;
}

/// Stub backend used until the real GDK-backed binding lands. Publishes
/// Unsupported health so the popover subtitle reflects the gap honestly.
/// Logs each toggle for observability.
pub struct NoopWaylandInhibitor;

impl WaylandIdleInhibitor for NoopWaylandInhibitor {
    fn set_inhibited(&mut self, inhibited: bool) {
        tracing::debug!(
            inhibited,
            "NoopWaylandInhibitor: set_inhibited (no compositor effect)"
        );
    }
    fn health(&self) -> WaylandHealth {
        WaylandHealth::Unsupported {
            message: "Wayland idle inhibit not yet bound".into(),
        }
    }
}

/// Long-running task. Observes state.inhibitors and toggles the backend
/// whenever the "any record has targets.idle" predicate flips. Binary
/// state — once on, additional idle records don't re-fire set_inhibited.
pub async fn run<B: WaylandIdleInhibitor + 'static>(
    mut backend: B,
    mut state_rx: watch::Receiver<State>,
    cancel: CancellationToken,
) {
    let mut current = false;
    loop {
        let next = state_rx.borrow().inhibitors.iter().any(|r| r.targets.idle);
        if next != current {
            backend.set_inhibited(next);
            current = next;
        }
        tokio::select! {
            _ = cancel.cancelled() => return,
            r = state_rx.changed() => if r.is_err() { return; },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::idle_inhibitor::{
        IdleInhibitorRecord, IdleInhibitorSource, InhibitionTargets,
    };
    use std::sync::{Arc, Mutex};

    struct FakeBackend {
        events: Arc<Mutex<Vec<bool>>>,
    }

    impl WaylandIdleInhibitor for FakeBackend {
        fn set_inhibited(&mut self, inhibited: bool) {
            self.events.lock().unwrap().push(inhibited);
        }
        fn health(&self) -> WaylandHealth {
            WaylandHealth::Ready
        }
    }

    fn rec_idle(id: u64) -> IdleInhibitorRecord {
        IdleInhibitorRecord {
            id,
            who: "x".into(),
            why: "y".into(),
            bus_name: ":1.1".into(),
            process_name: String::new(),
            source: IdleInhibitorSource::screen_saver(1),
            targets: InhibitionTargets::idle_only(),
            can_release: true,
            added_at_unix: 0,
        }
    }

    fn rec_suspend_only(id: u64) -> IdleInhibitorRecord {
        let mut t = InhibitionTargets::default();
        t.suspend = true;
        IdleInhibitorRecord {
            id,
            who: "x".into(),
            why: "y".into(),
            bus_name: ":1.1".into(),
            process_name: String::new(),
            source: IdleInhibitorSource::screen_saver(2),
            targets: t,
            can_release: true,
            added_at_unix: 0,
        }
    }

    #[tokio::test]
    async fn binary_state_flips_exactly_once_per_predicate_change() {
        let (tx, rx) = watch::channel(State::default());
        let events = Arc::new(Mutex::new(Vec::new()));
        let backend = FakeBackend { events: events.clone() };
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move {
            run(backend, rx, task_cancel).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert!(events.lock().unwrap().is_empty());

        let mut s = State::default();
        s.inhibitors.push(rec_idle(1));
        tx.send(s.clone()).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert_eq!(*events.lock().unwrap(), vec![true]);

        s.inhibitors.push(rec_idle(2));
        tx.send(s.clone()).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert_eq!(*events.lock().unwrap(), vec![true]);

        s.inhibitors.clear();
        s.inhibitors.push(rec_suspend_only(3));
        tx.send(s.clone()).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert_eq!(*events.lock().unwrap(), vec![true, false]);

        cancel.cancel();
        let _ = task.await;
    }

    #[test]
    fn noop_backend_reports_unsupported() {
        let b = NoopWaylandInhibitor;
        assert!(matches!(b.health(), WaylandHealth::Unsupported { .. }));
    }
}
