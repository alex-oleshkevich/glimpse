use std::{collections::HashMap, process::Stdio, time::Duration};

use glimpse_dbus::{idle::*, testing::PrivateBus};
use glimpsectl::{commands::idle::idle_inhibit_child, errors::ChildExit};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::mpsc,
};

struct Idle {
    events: mpsc::UnboundedSender<String>,
}

#[zbus::interface(name = "me.aresa.Glimpse.Idle1")]
impl Idle {
    fn hold(&self, seconds: u32) -> u64 {
        self.events.send(format!("hold:{seconds}")).unwrap();
        42
    }

    fn release(&self, id: u64) {
        self.events.send(format!("release:{id}")).unwrap();
    }

    #[zbus(property)]
    fn inhibitors(&self) -> Vec<IdleInhibitorRecord> {
        Vec::new()
    }

    #[zbus(property)]
    fn hold_expiries(&self) -> HashMap<u64, u64> {
        HashMap::new()
    }

    #[zbus(property)]
    fn health(&self) -> InhibitorsHealth {
        InhibitorsHealth::default()
    }
}

async fn provider(bus: &PrivateBus) -> (zbus::Connection, mpsc::UnboundedReceiver<String>) {
    let server = bus.connection().await;
    let (events, receiver) = mpsc::unbounded_channel();
    server
        .object_server()
        .at(GLIMPSE_IDLE_OBJECT_PATH, Idle { events })
        .await
        .unwrap();
    glimpse_dbus::own_name(&server, GLIMPSE_IDLE_BUS_NAME)
        .await
        .unwrap();
    (server, receiver)
}

#[tokio::test]
async fn inhibited_children_release_on_success_failure_signal_and_spawn_failure() {
    let bus = PrivateBus::start();
    let (_server, mut events) = provider(&bus).await;
    let client = bus.connection().await;
    for (script, expected) in [("exit 0", 0), ("exit 23", 23), ("kill -TERM $$", 143)] {
        let command = ["sh".into(), "-c".into(), script.into()];
        let result = idle_inhibit_child(&client, None, &command, true).await;
        if expected == 0 {
            result.unwrap();
        } else {
            assert_eq!(
                result.unwrap_err().downcast_ref::<ChildExit>().unwrap().0,
                expected
            );
        }
        assert_eq!(events.recv().await.unwrap(), "hold:0");
        assert_eq!(events.recv().await.unwrap(), "release:42");
    }
    assert!(
        idle_inhibit_child(&client, None, &["/no-such-inhibited-command".into()], false)
            .await
            .is_err()
    );
    assert_eq!(events.recv().await.unwrap(), "hold:0");
    assert_eq!(events.recv().await.unwrap(), "release:42");
}

#[tokio::test]
async fn interrupting_the_cli_forwards_the_signal_reaps_the_child_and_releases() {
    let bus = PrivateBus::start();
    let (_server, mut events) = provider(&bus).await;
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
        .args([
            "idle",
            "inhibit",
            "--",
            "sh",
            "-c",
            "printf 'ready\\n'; exec sleep 300",
        ])
        .env("DBUS_SESSION_BUS_ADDRESS", bus.address())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(8), lines.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        "ready"
    );
    unsafe {
        assert_eq!(libc::kill(child.id().unwrap() as i32, libc::SIGTERM), 0);
    }
    let status = tokio::time::timeout(Duration::from_secs(8), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status.code(), Some(143));
    assert_eq!(events.recv().await.unwrap(), "hold:0");
    assert_eq!(events.recv().await.unwrap(), "release:42");
}

#[tokio::test]
async fn timed_holds_are_explicitly_released() {
    let bus = PrivateBus::start();
    let (_server, mut events) = provider(&bus).await;
    let client = bus.connection().await;
    idle_inhibit_child(&client, Some(1), &[], true)
        .await
        .unwrap();
    assert_eq!(events.recv().await.unwrap(), "hold:1");
    assert_eq!(events.recv().await.unwrap(), "release:42");
}

#[tokio::test]
async fn an_inhibited_child_releases_its_original_provider_after_name_replacement() {
    let bus = PrivateBus::start();
    let (original, mut events) = provider(&bus).await;
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
        .args([
            "idle",
            "inhibit",
            "--",
            "sh",
            "-c",
            "printf 'ready\\n'; exec sleep 300",
        ])
        .env("DBUS_SESSION_BUS_ADDRESS", bus.address())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(8), lines.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        "ready"
    );
    assert_eq!(events.recv().await.unwrap(), "hold:0");
    original.release_name(GLIMPSE_IDLE_BUS_NAME).await.unwrap();
    let (_replacement, mut replacement_events) = provider(&bus).await;
    let client = bus.connection().await;
    assert_eq!(
        Idle1Proxy::new(&client)
            .await
            .unwrap()
            .hold(0)
            .await
            .unwrap(),
        42
    );
    assert_eq!(replacement_events.recv().await.unwrap(), "hold:0");
    unsafe {
        assert_eq!(libc::kill(child.id().unwrap() as i32, libc::SIGTERM), 0);
    }
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(8), child.wait())
            .await
            .unwrap()
            .unwrap()
            .code(),
        Some(143)
    );
    assert_eq!(
        replacement_events.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    );
    assert_eq!(events.try_recv().unwrap(), "release:42");
}
