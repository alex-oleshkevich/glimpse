use std::sync::OnceLock;

use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use glimpse_core::services::idle_inhibitor::{IdleInhibitorHandle, State};

/// Shell-only handles produced after we connect to the daemon's idle
/// inhibitor proxy. Populated once at shell startup; the panel applet
/// factory reads from it to wire the idle applet. If unset, the idle
/// applet is skipped (e.g. when the daemon is unavailable).
#[derive(Clone)]
pub struct ShellExtensions {
    pub idle_inhibitor: IdleInhibitorHandle,
    pub wayland_health: watch::Receiver<WaylandHealth>,
    pub own_unique_bus_name: String,
}

pub static SHELL_EXTENSIONS: OnceLock<ShellExtensions> = OnceLock::new();

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
pub trait WaylandIdleInhibitor: Send {
    fn set_inhibited(&mut self, inhibited: bool);
    fn health(&self) -> WaylandHealth;
}

/// Stub backend used until the real GDK-backed binding is installed.
/// Publishes Unsupported health so the popover subtitle reflects the gap
/// honestly. Logs each toggle for observability.
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

pub mod gdk_backend {
    //! Real wayland-client backend bound to the GDK display + a visible
    //! panel surface. Constructed on the GTK main thread, then shipped
    //! to the tokio runner via a mpsc swap channel.

    use super::{WaylandHealth, WaylandIdleInhibitor};
    use gdk4_wayland::prelude::WaylandSurfaceExtManual;
    use glib::object::Cast;
    use gtk4::prelude::{NativeExt, WidgetExt};
    use wayland_client::{
        Connection, Dispatch, Proxy, QueueHandle,
        globals::{GlobalListContents, registry_queue_init},
        protocol::{wl_registry, wl_surface},
    };
    use wayland_protocols::wp::idle_inhibit::zv1::client::{
        zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1,
        zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1,
    };

    pub struct GdkWaylandInhibitor {
        conn: Connection,
        qh: QueueHandle<DispatchState>,
        manager: ZwpIdleInhibitManagerV1,
        surface: wl_surface::WlSurface,
        inhibitor: Option<ZwpIdleInhibitorV1>,
    }

    struct DispatchState;

    impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for DispatchState {
        fn event(
            _: &mut Self,
            _: &wl_registry::WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ZwpIdleInhibitManagerV1, ()> for DispatchState {
        fn event(
            _: &mut Self,
            _: &ZwpIdleInhibitManagerV1,
            _: <ZwpIdleInhibitManagerV1 as Proxy>::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ZwpIdleInhibitorV1, ()> for DispatchState {
        fn event(
            _: &mut Self,
            _: &ZwpIdleInhibitorV1,
            _: <ZwpIdleInhibitorV1 as Proxy>::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl GdkWaylandInhibitor {
        /// Construct the backend by binding zwp_idle_inhibit_manager_v1
        /// against the GDK Wayland display and capturing the panel's
        /// wl_surface (which must already be realized).
        pub fn try_new(panel_window: &gtk4::Window) -> Result<Self, String> {
            let display = panel_window
                .display()
                .downcast::<gdk4_wayland::WaylandDisplay>()
                .map_err(|_| "GDK display is not Wayland".to_string())?;
            let conn = display
                .wl_display()
                .ok_or_else(|| "GDK wl_display unavailable".to_string())?
                .backend()
                .upgrade()
                .map(|backend| Connection::from_backend(backend))
                .ok_or_else(|| "GDK wayland backend gone".to_string())?;

            let (globals, queue) = registry_queue_init::<DispatchState>(&conn)
                .map_err(|e| format!("registry init failed: {e}"))?;
            let qh = queue.handle();
            let manager = globals
                .bind::<ZwpIdleInhibitManagerV1, _, _>(&qh, 1..=1, ())
                .map_err(|e| format!("zwp_idle_inhibit_manager_v1 missing: {e}"))?;

            let gdk_surface = panel_window
                .surface()
                .ok_or_else(|| "panel window has no GDK surface".to_string())?;
            let wl_surface = gdk_surface
                .downcast::<gdk4_wayland::WaylandSurface>()
                .map_err(|_| "panel surface is not Wayland".to_string())?
                .wl_surface()
                .ok_or_else(|| "panel wl_surface unavailable".to_string())?;

            Ok(Self {
                conn,
                qh,
                manager,
                surface: wl_surface,
                inhibitor: None,
            })
        }
    }

    impl WaylandIdleInhibitor for GdkWaylandInhibitor {
        fn set_inhibited(&mut self, inhibited: bool) {
            if inhibited && self.inhibitor.is_none() {
                let inh = self.manager.create_inhibitor(&self.surface, &self.qh, ());
                self.inhibitor = Some(inh);
                if let Err(error) = self.conn.flush() {
                    tracing::warn!(?error, "wayland flush after create_inhibitor failed");
                }
                tracing::debug!("GdkWaylandInhibitor: created zwp_idle_inhibitor_v1");
            } else if !inhibited {
                if let Some(inh) = self.inhibitor.take() {
                    inh.destroy();
                    if let Err(error) = self.conn.flush() {
                        tracing::warn!(?error, "wayland flush after destroy failed");
                    }
                    tracing::debug!("GdkWaylandInhibitor: destroyed zwp_idle_inhibitor_v1");
                }
            }
        }
        fn health(&self) -> WaylandHealth {
            WaylandHealth::Ready
        }
    }
}

/// Long-running task. Observes state.inhibitors and toggles the active
/// backend whenever the "any record has targets.idle" predicate flips.
/// Accepts a swap channel that hot-replaces the backend (used to upgrade
/// from Noop to a real GdkWaylandInhibitor once a panel is realized).
pub async fn run(
    mut backend: Box<dyn WaylandIdleInhibitor + Send>,
    mut state_rx: watch::Receiver<State>,
    mut swap_rx: mpsc::Receiver<Box<dyn WaylandIdleInhibitor + Send>>,
    health_tx: watch::Sender<WaylandHealth>,
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
            swap = swap_rx.recv() => match swap {
                Some(new_backend) => {
                    if current {
                        backend.set_inhibited(false);
                    }
                    backend = new_backend;
                    let _ = health_tx.send(backend.health());
                    if current {
                        backend.set_inhibited(true);
                    }
                }
                None => {}
            },
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
        let (_swap_tx, swap_rx) = mpsc::channel(1);
        let (health_tx, _health_rx) = watch::channel(WaylandHealth::Ready);
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move {
            run(Box::new(backend), rx, swap_rx, health_tx, task_cancel).await;
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

    #[tokio::test]
    async fn swap_replaces_backend_and_preserves_inhibited_state() {
        let (state_tx, state_rx) = watch::channel(State::default());
        let events_a = Arc::new(Mutex::new(Vec::new()));
        let events_b = Arc::new(Mutex::new(Vec::new()));
        let backend_a = FakeBackend { events: events_a.clone() };
        let backend_b = FakeBackend { events: events_b.clone() };
        let (swap_tx, swap_rx) = mpsc::channel(1);
        let (health_tx, mut health_rx) =
            watch::channel(WaylandHealth::Unsupported { message: "init".into() });
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move {
            run(Box::new(backend_a), state_rx, swap_rx, health_tx, task_cancel).await;
        });

        let mut s = State::default();
        s.inhibitors.push(rec_idle(1));
        state_tx.send(s.clone()).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert_eq!(*events_a.lock().unwrap(), vec![true]);

        swap_tx.send(Box::new(backend_b)).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        // Old backend told to release, new backend told to inhibit.
        assert_eq!(*events_a.lock().unwrap(), vec![true, false]);
        assert_eq!(*events_b.lock().unwrap(), vec![true]);
        let _ = health_rx.changed().await;
        assert_eq!(*health_rx.borrow(), WaylandHealth::Ready);

        cancel.cancel();
        let _ = task.await;
    }
}
