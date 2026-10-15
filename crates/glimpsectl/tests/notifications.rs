use chrono::{DateTime, TimeZone as _, Timelike as _, Utc};
use chrono::{Local, NaiveTime, TimeDelta};
use glimpse_dbus::notifications::{NotificationRecord, NotificationUrgency};
use glimpsectl::commands::notifications::{INDEFINITELY, expiry, row};

struct NotificationCommands(tokio::sync::mpsc::UnboundedSender<(String, u32, String)>);

#[zbus::interface(name = "me.aresa.Glimpse.Notifications1")]
impl NotificationCommands {
    fn remove(&self, id: u32) {
        self.0.send(("remove".into(), id, String::new())).unwrap();
    }

    fn activate(&self, id: u32, activation_token: &str) {
        self.0
            .send(("activate".into(), id, activation_token.into()))
            .unwrap();
    }

    fn invoke_action(
        &self,
        id: u32,
        action_key: &str,
        activation_token: &str,
    ) -> zbus::fdo::Result<()> {
        assert!(activation_token.is_empty());
        if id != 42 || action_key != "open" {
            return Err(zbus::fdo::Error::InvalidArgs(
                "unknown notification or action".into(),
            ));
        }
        self.0
            .send(("action".into(), id, action_key.into()))
            .unwrap();
        Ok(())
    }
}

struct Sender(tokio::sync::mpsc::UnboundedSender<serde_json::Value>);

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Sender {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: Vec<String>,
        hints: std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
        expire_timeout: i32,
    ) -> u32 {
        self.0.send(serde_json::json!({
            "app": app_name, "replace": replaces_id, "icon": app_icon, "summary": summary, "body": body,
            "actions": actions, "urgency": u8::try_from(hints.get("urgency").unwrap()).unwrap(),
            "image": <&str>::try_from(hints.get("image-path").unwrap()).unwrap(), "expire": expire_timeout,
        })).unwrap();
        42
    }
}

#[tokio::test]
async fn notification_commands_use_typed_methods_and_preserve_backend_refusals() {
    use glimpse_dbus::{notifications::*, testing::PrivateBus};
    use glimpsectl::{
        commands::notifications::*,
        errors::{Exit, exit},
    };
    let bus = PrivateBus::start();
    let server = bus.connection().await;
    let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
    server
        .object_server()
        .at(
            GLIMPSE_NOTIFICATIONS_OBJECT_PATH,
            NotificationCommands(events),
        )
        .await
        .unwrap();
    glimpse_dbus::own_name(&server, GLIMPSE_NOTIFICATIONS_BUS_NAME)
        .await
        .unwrap();
    let client = bus.connection().await;
    notifications_remove(&client, 42).await.unwrap();
    notifications_activate(&client, 42).await.unwrap();
    notifications_action(&client, 42, "open").await.unwrap();
    assert_eq!(
        received.recv().await.unwrap(),
        ("remove".into(), 42, String::new())
    );
    assert_eq!(
        received.recv().await.unwrap(),
        ("activate".into(), 42, String::new())
    );
    assert_eq!(
        received.recv().await.unwrap(),
        ("action".into(), 42, "open".into())
    );
    assert_eq!(
        exit(
            &notifications_action(&client, 42, "absent")
                .await
                .unwrap_err()
        ),
        Exit::Rejected
    );
    assert_eq!(
        exit(&notifications_action(&client, 99, "open").await.unwrap_err()),
        Exit::Rejected
    );
}

#[tokio::test]
async fn send_preserves_freedesktop_fields_and_returns_only_the_created_id() {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let server = bus.connection().await;
    let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
    server
        .object_server()
        .at("/org/freedesktop/Notifications", Sender(events))
        .await
        .unwrap();
    glimpse_dbus::own_name(&server, "org.freedesktop.Notifications")
        .await
        .unwrap();
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(8),
        tokio::process::Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
            .args([
                "--json",
                "notifications",
                "send",
                "Summary",
                "--body",
                "Body",
                "--app-name",
                "Test",
                "--icon",
                "mail",
                "--image",
                "/image.png",
                "--urgency",
                "critical",
                "--action",
                "open=Open",
                "--action",
                "close=Close",
                "--replace",
                "12",
                "--expire",
                "3500",
            ])
            .env("DBUS_SESSION_BUS_ADDRESS", bus.address())
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::json!({"id": 42})
    );
    assert_eq!(
        received.recv().await.unwrap(),
        serde_json::json!({
            "app": "Test", "replace": 12, "icon": "mail", "summary": "Summary", "body": "Body",
            "actions": ["open", "Open", "close", "Close"], "urgency": 2, "image": "/image.png", "expire": 3500,
        })
    );
    let invalid = tokio::process::Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
        .args([
            "--json",
            "notifications",
            "send",
            "Summary",
            "--action",
            "invalid",
        ])
        .env("DBUS_SESSION_BUS_ADDRESS", bus.address())
        .output()
        .await
        .unwrap();
    assert_eq!(invalid.status.code(), Some(4));
    assert!(invalid.stdout.is_empty());
    assert!(received.try_recv().is_err());
}

#[test]
fn an_expiry_is_the_next_time_the_clock_reads_that_way() {
    for (hour, minute) in [(0, 0), (7, 30), (12, 0), (23, 59)] {
        let time = NaiveTime::from_hms_opt(hour, minute, 0).expect("a valid time");
        let micros = expiry(true, Some(time)).expect("it arrives within two days");
        let when = DateTime::from_timestamp_micros(micros)
            .expect("the provider can read it back")
            .with_timezone(&Local);

        assert!(when > Local::now(), "{time} resolved into the past");
        assert!(when < Local::now() + TimeDelta::days(2));
        assert_eq!((when.hour(), when.minute()), (hour, minute));
    }
}

#[test]
fn no_expiry_is_the_sentinel_the_provider_reads_as_indefinite() {
    assert_eq!(expiry(true, None).expect("on, forever"), INDEFINITELY);
    assert_eq!(expiry(false, None).expect("off"), INDEFINITELY);
}

#[test]
fn an_expiry_on_a_command_turning_it_off_is_refused_rather_than_ignored() {
    let time = NaiveTime::from_hms_opt(21, 30, 0).expect("a valid time");
    let error = expiry(false, Some(time)).expect_err("there is nothing to lapse");

    assert!(error.to_string().contains("dnd on"), "{error}");
}

fn at(hour: u32, minute: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 14, hour, minute, 0)
        .single()
        .expect("a valid instant")
}

#[test]
fn an_unread_notification_is_not_dimmed_and_a_read_one_is() {
    let mut record = NotificationRecord {
        id: 7,
        app_id: "org.example.App".to_owned(),
        app_name: String::new(),
        app_pid: None,
        summary: "Build finished".to_owned(),
        body: None,
        icon: None,
        image: None,
        urgency: NotificationUrgency::Normal,
        actions: Vec::new(),
        progress: None,
        created: at(20, 0),
        unread: true,
        resident: false,
        expire_timeout: -1,
    };

    let [id, app, _, summary] = row(&record);
    assert_eq!(id, "7");
    assert_eq!(
        app, "org.example.App",
        "a nameless sender falls back to its id"
    );
    assert_eq!(summary, "Build finished");

    record.unread = false;
    let [_, _, _, dimmed] = row(&record);
    assert!(dimmed.contains("Build finished"));
    assert_ne!(dimmed, "Build finished", "a read summary is styled");
}
