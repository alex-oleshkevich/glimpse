use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{Mutex, mpsc, watch};
use tokio_util::sync::CancellationToken;

use glimpse_core::services::framework::{ServiceCommand, ServiceHandle};
use glimpse_core::services::idle_inhibitor::{
    Command, IdleInhibitorHandle, IdleInhibitorRecord, InhibitorsHealth, State,
};

#[zbus::proxy(
    interface = "me.aresa.GlimpseIdle.Inhibitors",
    default_service = "me.aresa.GlimpseIdle",
    default_path = "/me/aresa/GlimpseIdle/Inhibitors"
)]
pub trait Inhibitors {
    #[zbus(property)]
    fn inhibitors(&self) -> zbus::Result<Vec<IdleInhibitorRecord>>;

    #[zbus(property)]
    fn health(&self) -> zbus::Result<InhibitorsHealth>;

    fn release(&self, id: u64) -> zbus::Result<()>;
}

// Targets our daemon's well-known name directly instead of the cross-desktop
// org.freedesktop.ScreenSaver name. On GNOME/KDE that well-known name is
// owned by gsd-power / kscreensaver / etc., so a generic ScreenSaver call
// would go to them and never reach our daemon. By targeting me.aresa.GlimpseIdle
// + /ScreenSaver we hit the daemon's local ScreenSaver server unconditionally.
#[zbus::proxy(
    interface = "org.freedesktop.ScreenSaver",
    default_service = "me.aresa.GlimpseIdle",
    default_path = "/ScreenSaver"
)]
pub trait ScreenSaverClient {
    fn inhibit(&self, application_name: &str, reason: &str) -> zbus::Result<u32>;
    fn un_inhibit(&self, cookie: u32) -> zbus::Result<()>;
}

/// Tracks the cookie returned by ScreenSaver.Inhibit for the shell's own
/// manual hold so SetManualHold(false) can call UnInhibit(cookie). Lives
/// only in this process — the daemon never sees it.
struct ManualHoldState {
    cookie: Option<u32>,
}

/// Connect to the two proxies, subscribe to PropertiesChanged on the
/// Inhibitors API, and return a ServiceHandle the applet can consume.
pub async fn spawn(
    session: zbus::Connection,
    cancel: CancellationToken,
) -> Result<IdleInhibitorHandle> {
    let inhibitors_proxy = InhibitorsProxy::new(&session).await?;
    let screen_saver_proxy = ScreenSaverClientProxy::new(&session).await?;

    let unique_name = session
        .unique_name()
        .map(|n| n.to_string())
        .unwrap_or_default();
    tracing::info!(
        own_unique_name = %unique_name,
        "idle_inhibitors: client proxies created"
    );

    let initial = match read_state(&inhibitors_proxy).await {
        Ok(s) => {
            tracing::info!(
                count = s.inhibitors.len(),
                "idle_inhibitors: initial state loaded"
            );
            s
        }
        Err(e) => {
            tracing::warn!(error = ?e, "initial Inhibitors read failed; starting empty");
            State::default()
        }
    };
    let (state_tx, state_rx) = watch::channel(initial);
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<ServiceCommand<Command>>(16);
    let manual = Arc::new(Mutex::new(ManualHoldState { cookie: None }));

    let mirror_proxy = inhibitors_proxy.clone();
    let mirror_state_tx = state_tx.clone();
    let mirror_cancel = cancel.clone();
    tokio::spawn(async move {
        use futures_util::StreamExt;
        let mut inhibitors_changed = mirror_proxy.receive_inhibitors_changed().await;
        let mut health_changed = mirror_proxy.receive_health_changed().await;
        tracing::info!("idle_inhibitors: mirror task subscribed to PropertiesChanged");
        loop {
            tokio::select! {
                _ = mirror_cancel.cancelled() => return,
                Some(_) = inhibitors_changed.next() => {
                    tracing::info!("idle_inhibitors: Inhibitors PropertiesChanged → refreshing");
                    refresh(&mirror_proxy, &mirror_state_tx).await;
                }
                Some(_) = health_changed.next() => {
                    tracing::info!("idle_inhibitors: Health PropertiesChanged → refreshing");
                    refresh(&mirror_proxy, &mirror_state_tx).await;
                }
            }
        }
    });

    let cmd_screen_saver = screen_saver_proxy.clone();
    let cmd_inhibitors = inhibitors_proxy.clone();
    let cmd_manual = manual.clone();
    let cmd_cancel = cancel.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cmd_cancel.cancelled() => return,
                Some(cmd) = cmd_rx.recv() => {
                    if let ServiceCommand::Command(cmd) = cmd {
                        handle_command(cmd, &cmd_screen_saver, &cmd_inhibitors, &cmd_manual).await;
                    }
                }
            }
        }
    });

    Ok(ServiceHandle::new(state_rx, cmd_tx))
}

async fn read_state(proxy: &InhibitorsProxy<'_>) -> zbus::Result<State> {
    let inhibitors = proxy.inhibitors().await?;
    let health = proxy.health().await?;
    Ok(State { inhibitors, health })
}

async fn refresh(proxy: &InhibitorsProxy<'_>, tx: &watch::Sender<State>) {
    if let Ok(state) = read_state(proxy).await {
        let _ = tx.send(state);
    }
}

async fn handle_command(
    cmd: Command,
    screen_saver: &ScreenSaverClientProxy<'_>,
    inhibitors: &InhibitorsProxy<'_>,
    manual: &Mutex<ManualHoldState>,
) {
    match cmd {
        Command::SetManualHold(true) => {
            tracing::info!("idle_inhibitors: sending ScreenSaver.Inhibit('Glimpse', 'Manual hold')");
            match screen_saver.inhibit("Glimpse", "Manual hold").await {
                Ok(cookie) => {
                    tracing::info!(cookie, "idle_inhibitors: ScreenSaver.Inhibit returned cookie");
                    manual.lock().await.cookie = Some(cookie);
                }
                Err(e) => tracing::warn!(error = ?e, "manual hold Inhibit failed"),
            }
        }
        Command::SetManualHold(false) => {
            let cookie = manual.lock().await.cookie.take();
            if let Some(cookie) = cookie {
                tracing::info!(cookie, "idle_inhibitors: sending ScreenSaver.UnInhibit");
                if let Err(e) = screen_saver.un_inhibit(cookie).await {
                    tracing::warn!(error = ?e, cookie, "manual hold UnInhibit failed");
                }
            } else {
                tracing::info!("idle_inhibitors: SetManualHold(false) with no tracked cookie");
            }
        }
        Command::Release { id } => {
            tracing::info!(id, "idle_inhibitors: sending Inhibitors.Release");
            if let Err(e) = inhibitors.release(id).await {
                tracing::warn!(error = ?e, id, "Release failed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::idle_inhibitor::{IdleInhibitorSource, InhibitionTargets};

    fn rec(id: u64, source: IdleInhibitorSource, bus_name: &str) -> IdleInhibitorRecord {
        IdleInhibitorRecord {
            id,
            who: "x".into(),
            why: "y".into(),
            bus_name: bus_name.into(),
            process_name: String::new(),
            source,
            targets: InhibitionTargets::idle_only(),
            can_release: true,
            added_at_unix: 0,
        }
    }

    #[test]
    fn record_constructor_compiles() {
        let _r = rec(1, IdleInhibitorSource::screen_saver(9), ":1.1");
    }
}
