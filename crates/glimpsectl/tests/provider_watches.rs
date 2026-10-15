use std::{
    collections::HashMap,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::Duration,
};

use glimpse_dbus::{idle::*, night_light::*, notifications::*, settings::*, testing::PrivateBus};
use tokio::io::{AsyncBufReadExt, BufReader};

struct Notifications {
    value: Arc<AtomicU32>,
    first: AtomicBool,
    connection: zbus::Connection,
}

#[zbus::interface(name = "me.aresa.Glimpse.Notifications1")]
impl Notifications {
    #[zbus(property)]
    async fn snapshot(&self) -> NotificationsSnapshot {
        let value = self.value.load(Ordering::SeqCst);
        if self.first.swap(false, Ordering::SeqCst) {
            self.value.store(1, Ordering::SeqCst);
            invalidate(
                &self.connection,
                GLIMPSE_NOTIFICATIONS_OBJECT_PATH,
                "me.aresa.Glimpse.Notifications1",
                "Snapshot",
            )
            .await;
        }
        (Vec::new(), (false, 0), true, value.to_string())
    }
}

struct NightLight(Arc<AtomicU32>);

#[zbus::interface(name = "me.aresa.Glimpse.NightLight1")]
impl NightLight {
    #[zbus(property)]
    fn snapshot(&self) -> NightLightSnapshot {
        NightLightSnapshot {
            schedule: "automatic".into(),
            overridden: false,
            temperature: 6500 + self.0.load(Ordering::SeqCst),
            target: 4200,
            active: false,
            serving: true,
            reason: String::new(),
            configured: "automatic".into(),
            manual: false,
            paused_until: 0,
        }
    }
}

struct Settings(Arc<AtomicU32>);

#[zbus::interface(name = "me.aresa.Glimpse.Settings1")]
impl Settings {
    #[zbus(property)]
    fn mode(&self) -> ModeSnapshot {
        (
            true,
            false,
            0,
            1 + self.0.load(Ordering::SeqCst),
            0,
            String::new(),
        )
    }
}

struct Idle(Arc<AtomicU32>);

#[zbus::interface(name = "me.aresa.Glimpse.Idle1")]
impl Idle {
    #[zbus(property)]
    fn inhibitors(&self) -> Vec<IdleInhibitorRecord> {
        Vec::new()
    }

    #[zbus(property)]
    fn health(&self) -> InhibitorsHealth {
        let mut health = InhibitorsHealth::default();
        health.login1.message = self.0.load(Ordering::SeqCst).to_string();
        health
    }
}

async fn invalidate(connection: &zbus::Connection, path: &str, interface: &str, property: &str) {
    connection
        .emit_signal(
            None::<&str>,
            path,
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            &(
                interface,
                HashMap::<String, zbus::zvariant::OwnedValue>::new(),
                vec![property],
            ),
        )
        .await
        .unwrap();
}

async fn line(
    lines: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
) -> serde_json::Value {
    let line = tokio::time::timeout(Duration::from_secs(8), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    serde_json::from_str(&line).unwrap()
}

#[tokio::test]
async fn watches_subscribe_before_initial_read_follow_real_property_signals_and_exit_on_owner_loss()
{
    let bus = PrivateBus::start();
    let server = bus.connection().await;
    for (command, name, path, interface, property) in [
        (
            ["notifications", "list"],
            GLIMPSE_NOTIFICATIONS_BUS_NAME,
            GLIMPSE_NOTIFICATIONS_OBJECT_PATH,
            "me.aresa.Glimpse.Notifications1",
            "Snapshot",
        ),
        (
            ["night-light", "status"],
            GLIMPSE_NIGHT_LIGHT_BUS_NAME,
            GLIMPSE_NIGHT_LIGHT_OBJECT_PATH,
            "me.aresa.Glimpse.NightLight1",
            "Snapshot",
        ),
        (
            ["appearance", "status"],
            GLIMPSE_SETTINGS_BUS_NAME,
            GLIMPSE_SETTINGS_OBJECT_PATH,
            "me.aresa.Glimpse.Settings1",
            "Mode",
        ),
        (
            ["idle", "status"],
            GLIMPSE_IDLE_BUS_NAME,
            GLIMPSE_IDLE_OBJECT_PATH,
            "me.aresa.Glimpse.Idle1",
            "Health",
        ),
    ] {
        let value = Arc::new(AtomicU32::new(0));
        match command[0] {
            "notifications" => {
                server
                    .object_server()
                    .at(
                        path,
                        Notifications {
                            value: value.clone(),
                            first: AtomicBool::new(true),
                            connection: server.clone(),
                        },
                    )
                    .await
                    .unwrap();
            }
            "night-light" => {
                server
                    .object_server()
                    .at(path, NightLight(value.clone()))
                    .await
                    .unwrap();
            }
            "appearance" => {
                server
                    .object_server()
                    .at(path, Settings(value.clone()))
                    .await
                    .unwrap();
            }
            "idle" => {
                server
                    .object_server()
                    .at(path, Idle(value.clone()))
                    .await
                    .unwrap();
            }
            _ => unreachable!(),
        }
        glimpse_dbus::own_name(&server, name).await.unwrap();
        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
            .args(command)
            .args(["--watch", "--json"])
            .env("DBUS_SESSION_BUS_ADDRESS", bus.address())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let initial = line(&mut lines).await;
        if command[0] != "notifications" {
            value.store(1, Ordering::SeqCst);
            invalidate(&server, path, interface, property).await;
        }
        let changed = line(&mut lines).await;
        assert_ne!(initial, changed, "{command:?} never re-read its property");
        server.release_name(name).await.unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(8), child.wait())
                .await
                .unwrap()
                .unwrap()
                .code(),
            Some(3)
        );
    }
}

#[tokio::test]
async fn a_closed_watch_pipe_stops_successfully_after_the_next_change() {
    let bus = PrivateBus::start();
    let server = bus.connection().await;
    server
        .object_server()
        .at(
            GLIMPSE_SETTINGS_OBJECT_PATH,
            Settings(Arc::new(AtomicU32::new(0))),
        )
        .await
        .unwrap();
    glimpse_dbus::own_name(&server, GLIMPSE_SETTINGS_BUS_NAME)
        .await
        .unwrap();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
        .args(["appearance", "status", "--watch", "--json"])
        .env("DBUS_SESSION_BUS_ADDRESS", bus.address())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    line(&mut lines).await;
    drop(lines);
    invalidate(
        &server,
        GLIMPSE_SETTINGS_OBJECT_PATH,
        "me.aresa.Glimpse.Settings1",
        "Mode",
    )
    .await;
    assert!(
        tokio::time::timeout(Duration::from_secs(8), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
}

struct PendingSettings {
    blocked: Arc<AtomicBool>,
    requests: tokio::sync::mpsc::UnboundedSender<()>,
    unblock: Arc<tokio::sync::Notify>,
}

#[zbus::interface(name = "me.aresa.Glimpse.Settings1")]
impl PendingSettings {
    #[zbus(property)]
    async fn mode(
        &self,
        #[zbus(header)] header: Option<zbus::message::Header<'_>>,
    ) -> ModeSnapshot {
        assert!(
            header
                .unwrap()
                .destination()
                .unwrap()
                .as_str()
                .starts_with(':')
        );
        self.requests.send(()).unwrap();
        if self.blocked.load(Ordering::SeqCst) {
            self.unblock.notified().await;
        }
        (true, false, 0, 1, 0, String::new())
    }
}

struct ReplacementSettings(Arc<AtomicU32>);

#[zbus::interface(name = "me.aresa.Glimpse.Settings1")]
impl ReplacementSettings {
    #[zbus(property)]
    fn mode(&self) -> ModeSnapshot {
        self.0.fetch_add(1, Ordering::SeqCst);
        (true, false, 0, 2, 0, String::new())
    }
}

#[tokio::test]
async fn losing_the_original_owner_cancels_pending_initial_and_refresh_reads_without_querying_replacements()
 {
    for initial in [true, false] {
        let bus = PrivateBus::start();
        let server = bus.connection().await;
        let replacement = bus.connection().await;
        let blocked = Arc::new(AtomicBool::new(initial));
        let unblock = Arc::new(tokio::sync::Notify::new());
        let (requests, mut received) = tokio::sync::mpsc::unbounded_channel();
        server
            .object_server()
            .at(
                GLIMPSE_SETTINGS_OBJECT_PATH,
                PendingSettings {
                    blocked: blocked.clone(),
                    requests,
                    unblock: unblock.clone(),
                },
            )
            .await
            .unwrap();
        let replacement_reads = Arc::new(AtomicU32::new(0));
        replacement
            .object_server()
            .at(
                GLIMPSE_SETTINGS_OBJECT_PATH,
                ReplacementSettings(replacement_reads.clone()),
            )
            .await
            .unwrap();
        glimpse_dbus::own_name(&server, GLIMPSE_SETTINGS_BUS_NAME)
            .await
            .unwrap();
        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
            .args(["appearance", "status", "--watch", "--json"])
            .env("DBUS_SESSION_BUS_ADDRESS", bus.address())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        tokio::time::timeout(Duration::from_secs(8), received.recv())
            .await
            .unwrap()
            .unwrap();
        if !initial {
            line(&mut lines).await;
            blocked.store(true, Ordering::SeqCst);
            invalidate(
                &server,
                GLIMPSE_SETTINGS_OBJECT_PATH,
                "me.aresa.Glimpse.Settings1",
                "Mode",
            )
            .await;
            tokio::time::timeout(Duration::from_secs(8), received.recv())
                .await
                .unwrap()
                .unwrap();
        }
        server
            .release_name(GLIMPSE_SETTINGS_BUS_NAME)
            .await
            .unwrap();
        glimpse_dbus::own_name(&replacement, GLIMPSE_SETTINGS_BUS_NAME)
            .await
            .unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), child.wait())
                .await
                .unwrap()
                .unwrap()
                .code(),
            Some(3)
        );
        assert_eq!(replacement_reads.load(Ordering::SeqCst), 0);
        assert!(lines.next_line().await.unwrap().is_none());
        unblock.notify_one();
    }
}
