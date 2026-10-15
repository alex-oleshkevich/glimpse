use std::{
    collections::HashMap,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use glimpse_dbus::{gtk_actions::object_path, testing::PrivateBus};
use tokio::io::{AsyncBufReadExt, BufReader};
use zbus::zvariant::{OwnedValue, Signature, Str, Value};
use zbus::{
    export::futures_core::Stream,
    fdo::{DBusProxy, RequestNameFlags},
};

type Descriptions = HashMap<String, (bool, Signature, Vec<OwnedValue>)>;

fn descriptions(name: &str) -> Descriptions {
    let state = HashMap::from([
        ("connector", OwnedValue::from(Str::from("DP-1"))),
        ("position", OwnedValue::from(Str::from("top"))),
        ("zone", OwnedValue::from(Str::from("left"))),
        ("index", OwnedValue::from(0u32)),
        ("name", OwnedValue::from(Str::from(name))),
        ("alive", OwnedValue::from(true)),
        ("own-view", OwnedValue::from(false)),
        ("popover", OwnedValue::from(false)),
    ]);
    HashMap::from([(
        "applet.1".into(),
        (
            true,
            "(sv)".try_into().unwrap(),
            vec![OwnedValue::try_from(Value::from(state)).unwrap()],
        ),
    )])
}

struct Actions {
    blocked: bool,
    requests: tokio::sync::mpsc::UnboundedSender<String>,
    unblock: Arc<tokio::sync::Notify>,
    reads: Arc<AtomicUsize>,
}

#[zbus::interface(name = "org.gtk.Actions")]
impl Actions {
    async fn describe_all(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> Descriptions {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.requests
            .send(header.destination().unwrap().as_str().to_owned())
            .unwrap();
        if self.blocked {
            self.unblock.notified().await;
        }
        descriptions("clock")
    }
}

#[tokio::test]
async fn applet_watch_pins_its_owner_and_cancels_on_loss_or_atomic_replacement_even_during_initial_read()
 {
    for pending in [false, true] {
        for replace in [false, true] {
            let bus = PrivateBus::start();
            let server = bus.connection().await;
            let replacement = bus.connection().await;
            let client = bus.connection().await;
            let app_id = "me.test.Panel";
            let (requests, mut received) = tokio::sync::mpsc::unbounded_channel();
            let unblock = Arc::new(tokio::sync::Notify::new());
            let reads = Arc::new(AtomicUsize::new(0));
            server
                .object_server()
                .at(
                    object_path(app_id),
                    Actions {
                        blocked: pending,
                        requests: requests.clone(),
                        unblock: unblock.clone(),
                        reads: reads.clone(),
                    },
                )
                .await
                .unwrap();
            let replacement_reads = Arc::new(AtomicUsize::new(0));
            replacement
                .object_server()
                .at(
                    object_path(app_id),
                    Actions {
                        blocked: false,
                        requests,
                        unblock: unblock.clone(),
                        reads: replacement_reads.clone(),
                    },
                )
                .await
                .unwrap();
            DBusProxy::new(&server)
                .await
                .unwrap()
                .request_name(
                    app_id.try_into().unwrap(),
                    RequestNameFlags::AllowReplacement | RequestNameFlags::DoNotQueue,
                )
                .await
                .unwrap();
            let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
                .args(["applet", "list", "--watch", "--json", "--panel", app_id])
                .env("DBUS_SESSION_BUS_ADDRESS", bus.address())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .unwrap();
            let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
            let destination = tokio::time::timeout(Duration::from_secs(5), received.recv())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(destination, server.unique_name().unwrap().as_str());
            if !pending {
                let initial: serde_json::Value = serde_json::from_str(
                    &tokio::time::timeout(Duration::from_secs(5), lines.next_line())
                        .await
                        .unwrap()
                        .unwrap()
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(initial["name"], "clock");
                assert_eq!(initial["zone"], "left");
                let updated = descriptions("updated-clock")
                    .remove("applet.1")
                    .unwrap()
                    .2
                    .remove(0);
                server
                    .emit_signal(
                        None::<&str>,
                        object_path(app_id),
                        "org.gtk.Actions",
                        "Changed",
                        &(
                            Vec::<String>::new(),
                            HashMap::<String, bool>::new(),
                            HashMap::from([("applet.1", updated)]),
                            Descriptions::new(),
                        ),
                    )
                    .await
                    .unwrap();
                let delta: serde_json::Value = serde_json::from_str(
                    &tokio::time::timeout(Duration::from_secs(5), lines.next_line())
                        .await
                        .unwrap()
                        .unwrap()
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(delta["name"], "updated-clock");
                server
                    .emit_signal(
                        None::<&str>,
                        object_path(app_id),
                        "org.gtk.Actions",
                        "Changed",
                        &(
                            vec!["applet.1"],
                            HashMap::<String, bool>::new(),
                            HashMap::<String, OwnedValue>::new(),
                            Descriptions::new(),
                        ),
                    )
                    .await
                    .unwrap();
                let removed: serde_json::Value = serde_json::from_str(
                    &tokio::time::timeout(Duration::from_secs(5), lines.next_line())
                        .await
                        .unwrap()
                        .unwrap()
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(removed, serde_json::json!({"removed": "applet.1"}));
            }
            let dbus = DBusProxy::new(&client).await.unwrap();
            let mut owners = Box::pin(
                dbus.receive_name_owner_changed_with_args(&[(0, app_id)])
                    .await
                    .unwrap(),
            );
            if replace {
                DBusProxy::new(&replacement)
                    .await
                    .unwrap()
                    .request_name(
                        app_id.try_into().unwrap(),
                        RequestNameFlags::ReplaceExisting | RequestNameFlags::DoNotQueue,
                    )
                    .await
                    .unwrap();
            } else {
                DBusProxy::new(&server)
                    .await
                    .unwrap()
                    .release_name(app_id.try_into().unwrap())
                    .await
                    .unwrap();
            }
            let owner_change = tokio::time::timeout(
                Duration::from_secs(5),
                std::future::poll_fn(|cx| owners.as_mut().poll_next(cx)),
            )
            .await
            .unwrap()
            .unwrap();
            let args = owner_change.args().unwrap();
            assert_eq!(
                args.old_owner().as_ref().unwrap().as_str(),
                server.unique_name().unwrap().as_str()
            );
            assert_eq!(args.new_owner().is_some(), replace);
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(2), child.wait())
                    .await
                    .unwrap()
                    .unwrap()
                    .code(),
                Some(3)
            );
            assert_eq!(reads.load(Ordering::SeqCst), 1);
            assert_eq!(replacement_reads.load(Ordering::SeqCst), 0);
            assert!(lines.next_line().await.unwrap().is_none());
            unblock.notify_one();
        }
    }
}
