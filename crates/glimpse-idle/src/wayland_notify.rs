use std::os::fd::AsFd;
use std::time::Duration;

use tokio::io::{Interest, unix::AsyncFd};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, delegate_noop};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1::{self, ExtIdleNotificationV1},
    ext_idle_notifier_v1::ExtIdleNotifierV1,
};

use crate::idle;

pub const WAYLAND_SETUP_TIMEOUT: Duration = Duration::from_secs(5);
const WAYLAND_RETRY_DELAY: Duration = Duration::from_secs(2);

pub async fn run(idle: idle::Handle, cancel: CancellationToken) {
    let mut connecting: Option<JoinHandle<anyhow::Result<Setup>>> = None;

    loop {
        match run_inner(
            idle.clone(),
            cancel.clone(),
            WAYLAND_SETUP_TIMEOUT,
            &mut connecting,
        )
        .await
        {
            Ok(()) => break,
            Err(error) => {
                tracing::warn!(%error, "idle wayland backend failed");
                send_health(
                    &idle,
                    idle::Health::Degraded {
                        message: error.to_string(),
                    },
                );
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = tokio::time::sleep(WAYLAND_RETRY_DELAY) => {}
                }
            }
        }
    }
}

struct Setup {
    conn: Connection,
    event_queue: wayland_client::EventQueue<Backend>,
    notifier: ExtIdleNotifierV1,
    seat: wl_seat::WlSeat,
    qh: QueueHandle<Backend>,
    backend: Backend,
}

async fn run_inner(
    idle: idle::Handle,
    cancel: CancellationToken,
    setup_timeout: Duration,
    connecting: &mut Option<JoinHandle<anyhow::Result<Setup>>>,
) -> anyhow::Result<()> {
    let Setup {
        conn,
        mut event_queue,
        notifier,
        seat,
        qh,
        mut backend,
    } = tokio::select! {
        _ = cancel.cancelled() => return Ok(()),
        setup = with_timeout(setup_timeout, connect_blocking(idle.clone(), connecting)) => setup?,
    };

    tracing::info!("idle backend connected to ext-idle-notify-v1");
    send_health(&idle, idle::Health::Ready);

    let mut state_rx = idle.subscribe();
    sync_notifications(&mut backend, &notifier, &seat, &qh, &idle.snapshot());
    conn.flush()?;

    let owned_fd = conn.as_fd().try_clone_to_owned()?;
    let async_fd = AsyncFd::with_interest(owned_fd, Interest::READABLE)?;

    loop {
        event_queue.dispatch_pending(&mut backend)?;
        conn.flush()?;

        tokio::select! {
            _ = cancel.cancelled() => {
                clear_notifications(&mut backend);
                let _ = conn.flush();
                return Ok(());
            }
            changed = state_rx.changed() => {
                if changed.is_err() {
                    clear_notifications(&mut backend);
                    let _ = conn.flush();
                    return Ok(());
                }
                let snapshot = state_rx.borrow().clone();
                if backend.generation != Some(snapshot.generation) {
                    sync_notifications(&mut backend, &notifier, &seat, &qh, &snapshot);
                    conn.flush()?;
                }
            }
            readable = async_fd.readable() => {
                let mut guard = readable?;
                if let Some(read_guard) = conn.prepare_read() {
                    read_guard.read()?;
                }
                guard.clear_ready();
            }
        }
    }
}

async fn connect_blocking(
    idle: idle::Handle,
    connecting: &mut Option<JoinHandle<anyhow::Result<Setup>>>,
) -> anyhow::Result<Setup> {
    let handle =
        connecting.get_or_insert_with(|| tokio::task::spawn_blocking(move || connect_sync(idle)));
    let result = handle.await;
    *connecting = None;
    result.map_err(|error| anyhow::anyhow!("wayland setup worker failed: {error}"))?
}

fn connect_sync(idle: idle::Handle) -> anyhow::Result<Setup> {
    let conn = Connection::connect_to_env()?;
    let (globals, mut event_queue) = registry_queue_init::<Backend>(&conn)?;
    let qh = event_queue.handle();
    let notifier = globals.bind::<ExtIdleNotifierV1, _, _>(&qh, 1..=2, ())?;
    let seat = globals.bind::<wl_seat::WlSeat, _, _>(&qh, 1..=9, ())?;
    let mut backend = Backend {
        idle,
        notifications: vec![],
        generation: None,
    };
    event_queue.roundtrip(&mut backend)?;
    Ok(Setup {
        conn,
        event_queue,
        notifier,
        seat,
        qh,
        backend,
    })
}

async fn with_timeout<F, T>(timeout: Duration, future: F) -> anyhow::Result<T>
where
    F: std::future::Future<Output = anyhow::Result<T>>,
{
    match tokio::time::timeout(timeout, future).await {
        Ok(result) => result,
        Err(_) => Err(anyhow::anyhow!(
            "timed out connecting to ext-idle-notify-v1 after {timeout:?}"
        )),
    }
}

struct Backend {
    idle: idle::Handle,
    notifications: Vec<ExtIdleNotificationV1>,
    generation: Option<u64>,
}

fn sync_notifications(
    backend: &mut Backend,
    notifier: &ExtIdleNotifierV1,
    seat: &wl_seat::WlSeat,
    qh: &QueueHandle<Backend>,
    state: &idle::State,
) {
    clear_notifications(backend);

    if !state.enabled {
        backend.generation = Some(state.generation);
        return;
    }

    for listener in &state.listeners {
        backend
            .notifications
            .push(register_listener(notifier, seat, qh, listener));
    }

    tracing::info!(
        generation = state.generation,
        listeners = backend.notifications.len(),
        "idle backend registered listeners"
    );
    backend.generation = Some(state.generation);
}

fn register_listener(
    notifier: &ExtIdleNotifierV1,
    seat: &wl_seat::WlSeat,
    qh: &QueueHandle<Backend>,
    listener: &idle::ActiveListener,
) -> ExtIdleNotificationV1 {
    let timeout_ms = listener.timeout.saturating_mul(1000).min(u32::MAX as u64) as u32;

    if !listener.respect_inhibitors {
        if notifier.version() >= 2 {
            return notifier.get_input_idle_notification(timeout_ms, seat, qh, listener.id);
        }
        tracing::warn!(
            listener = listener.id,
            "idle backend cannot bypass inhibitors because ext-idle-notify-v1 v2 is unavailable"
        );
    }
    notifier.get_idle_notification(timeout_ms, seat, qh, listener.id)
}

fn clear_notifications(backend: &mut Backend) {
    for notification in backend.notifications.drain(..) {
        notification.destroy();
    }
}

fn send_health(idle: &idle::Handle, health: idle::Health) {
    if let Err(error) = idle.try_send(idle::Event::BackendHealth(health)) {
        tracing::warn!(%error, "failed to update idle backend health");
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Backend {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtIdleNotificationV1, usize> for Backend {
    fn event(
        state: &mut Self,
        _proxy: &ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event,
        listener: &usize,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let generation = state.generation.unwrap_or(0);
        let event = match event {
            ext_idle_notification_v1::Event::Idled => idle::Event::ListenerIdle {
                generation,
                id: *listener,
            },
            ext_idle_notification_v1::Event::Resumed => idle::Event::ListenerResume {
                generation,
                id: *listener,
            },
            _ => return,
        };
        if let Err(error) = state.idle.try_send(event) {
            tracing::warn!(listener, %error, "failed to forward idle backend event");
        }
    }
}

delegate_noop!(Backend: ignore wl_seat::WlSeat);
delegate_noop!(Backend: ignore ExtIdleNotifierV1);

#[cfg(test)]
mod tests {
    use super::*;

    /// AC-6: the setup helper surfaces a timeout rather than hanging.
    #[tokio::test]
    async fn setup_timeout_surfaces_error_for_a_stalled_connect() {
        let stalled = std::future::pending::<anyhow::Result<()>>();
        let error = with_timeout(Duration::from_millis(10), stalled)
            .await
            .expect_err("a never-completing setup must time out");
        assert!(
            error.to_string().contains("timed out"),
            "unexpected error message: {error}"
        );
    }

    #[tokio::test]
    async fn setup_timeout_passes_through_immediate_success() {
        let result = with_timeout(Duration::from_millis(10), async { Ok(7u32) }).await;
        assert_eq!(result.expect("immediate success should pass through"), 7);
    }
}
