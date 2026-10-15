use glimpse_dbus::{gtk_actions::object_path, own_name, testing::PrivateBus};
use std::{collections::HashMap, process::Stdio, time::Duration};
use tokio::io::{AsyncBufReadExt, BufReader};
use zbus::zvariant::{OwnedValue, Signature, Value};

type Descriptions = HashMap<String, (bool, Signature, Vec<OwnedValue>)>;

struct Actions(std::sync::atomic::AtomicBool);

#[zbus::interface(name = "org.gtk.Actions")]
impl Actions {
    async fn describe_all(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> Descriptions {
        let first = self.0.swap(false, std::sync::atomic::Ordering::Relaxed);
        if first {
            connection
                .emit_signal(
                    None::<&str>,
                    header.path().unwrap(),
                    "org.gtk.Actions",
                    "Changed",
                    &(
                        Vec::<String>::new(),
                        HashMap::<String, bool>::new(),
                        HashMap::<String, OwnedValue>::new(),
                        Descriptions::new(),
                    ),
                )
                .await
                .unwrap();
        }
        let bars: Vec<(u32, &str, &str)> = if first {
            Vec::new()
        } else {
            vec![(2, "DP-1", "bottom")]
        };
        HashMap::from([
            (
                "version".into(),
                (
                    false,
                    "".try_into().unwrap(),
                    vec![
                        OwnedValue::try_from(Value::from(if first { "first" } else { "second" }))
                            .unwrap(),
                    ],
                ),
            ),
            (
                "bars".into(),
                (
                    false,
                    "".try_into().unwrap(),
                    vec![OwnedValue::try_from(Value::from(bars)).unwrap()],
                ),
            ),
        ])
    }
}

#[tokio::test]
async fn watch_observes_changes_during_initial_snapshot_and_discovers_owner_removal() {
    let bus = PrivateBus::start();
    let server = bus.connection().await;
    let app_id = "me.test.Panel";
    server
        .object_server()
        .at(object_path(app_id), Actions(true.into()))
        .await
        .unwrap();
    own_name(&server, app_id).await.unwrap();
    own_name(&server, "me.test.Panel.AppletHost").await.unwrap();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
        .args(["panel", "list", "--watch", "--json"])
        .env("DBUS_SESSION_BUS_ADDRESS", bus.address())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let first: serde_json::Value = serde_json::from_str(
        &tokio::time::timeout(Duration::from_secs(5), lines.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(first[0]["version"], "first");
    assert_eq!(first[0]["bars"], serde_json::json!([]));
    let second: serde_json::Value = serde_json::from_str(
        &tokio::time::timeout(Duration::from_secs(5), lines.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(second[0]["version"], "second");
    assert_eq!(
        second[0]["bars"],
        serde_json::json!([{"index": 2, "connector": "DP-1", "position": "bottom"}])
    );
    assert_eq!(second[0]["applet_count"], 0);
    server.release_name(app_id).await.unwrap();
    let removed: serde_json::Value = serde_json::from_str(
        &tokio::time::timeout(Duration::from_secs(5), lines.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(removed, serde_json::json!([]));
    own_name(&server, app_id).await.unwrap();
    let restored: serde_json::Value = serde_json::from_str(
        &tokio::time::timeout(Duration::from_secs(5), lines.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(restored[0]["app_id"], app_id);
    let next_app_id = "me.test.NextPanel";
    server
        .object_server()
        .at(object_path(next_app_id), Actions(false.into()))
        .await
        .unwrap();
    own_name(&server, "me.test.NextPanel.AppletHost")
        .await
        .unwrap();
    let host_only: serde_json::Value = serde_json::from_str(
        &tokio::time::timeout(Duration::from_secs(5), lines.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(host_only.as_array().unwrap().len(), 1);
    own_name(&server, next_app_id).await.unwrap();
    let acquired: serde_json::Value = serde_json::from_str(
        &tokio::time::timeout(Duration::from_secs(5), lines.next_line())
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(acquired.as_array().unwrap().len(), 2);
    assert!(
        acquired
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["app_id"] == next_app_id)
    );
    child.kill().await.unwrap();
    child.wait().await.unwrap();
}

#[tokio::test]
async fn reload_signal_target_child() {
    if std::env::var_os("GLIMPSE_RELOAD_TEST_CHILD").is_none() {
        return;
    }
    use std::io::Write;
    let mut signal =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup()).unwrap();
    let connection = zbus::Connection::session().await.unwrap();
    own_name(&connection, "me.test.ReloadPanel").await.unwrap();
    println!("READY");
    std::io::stdout().flush().unwrap();
    tokio::time::timeout(Duration::from_secs(10), signal.recv())
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn reload_signals_only_the_selected_bus_owner_process() {
    let bus = PrivateBus::start();
    let mut target = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "reload_signal_target_child", "--nocapture"])
        .env("GLIMPSE_RELOAD_TEST_CHILD", "1")
        .env("DBUS_SESSION_BUS_ADDRESS", bus.address())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(target.stdout.take().unwrap()).lines();
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(line) = lines.next_line().await.unwrap() {
            if line == "READY" {
                return;
            }
        }
        panic!("signal target exited before owning its bus name");
    })
    .await
    .unwrap();
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
        .args([
            "panel",
            "reload",
            "--panel",
            "me.test.ReloadPanel",
            "--json",
        ])
        .env("DBUS_SESSION_BUS_ADDRESS", bus.address())
        .output()
        .await
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty());
    assert!(
        tokio::time::timeout(Duration::from_secs(5), target.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
}
