use std::collections::HashMap;
use std::time::Duration;

use futures_util::StreamExt;
use glimpse_config::Config;
use glimpse_dbus::Buses;
use glimpse_dbus::freedesktop_notifications::FreedesktopNotificationsProxy;
use glimpse_dbus::testing::PrivateBus;
use glimpse_services::{
    Incoming, Notifications, NotificationsHandle, Running, ServiceState, UNREAD_MAX,
};
use zbus::zvariant::Value;
use zbus::{MatchRule, MessageStream, message::Type};

const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";

async fn notifications() -> (PrivateBus, Running<Notifications>, NotificationsHandle) {
    let bus = PrivateBus::start();
    let (service, handle) = Running::<Notifications>::spawn(
        &Config::default(),
        Buses::for_session(bus.connection().await),
        (),
    );
    let mut health = handle.health();
    tokio::time::timeout(
        Duration::from_secs(20),
        health.wait_for(|state| matches!(state, ServiceState::Running)),
    )
    .await
    .expect("the server comes up")
    .expect("the service is alive");
    (bus, service, handle)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn activation_hands_over_the_token_before_reporting_the_default_action() {
    let (bus, mut service, handle) = notifications().await;
    let client = bus.connection().await;
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .interface(NAME)
        .expect("a valid interface")
        .path(PATH)
        .expect("a valid path")
        .build();
    let mut signals = MessageStream::for_match_rule(rule, &client, None)
        .await
        .expect("a signal stream");

    let reply = client
        .call_method(
            Some(NAME),
            PATH,
            Some(NAME),
            "Notify",
            &(
                "Chat",
                0u32,
                "",
                "Marta",
                "Hello",
                vec!["default", "Open"],
                HashMap::<&str, Value<'_>>::new(),
                -1i32,
            ),
        )
        .await
        .expect("notify answers");
    let id: u32 = reply.body().deserialize().expect("an id");
    handle
        .subscribe()
        .wait_for(|state| {
            state
                .list
                .as_ref()
                .is_some_and(|list| list.notifications.iter().any(|record| record.id == id))
        })
        .await
        .expect("the notification is held");

    handle
        .activate(id, Some("token-1".to_owned()))
        .await
        .expect("activation succeeds");

    let mut seen = Vec::new();
    while seen.len() < 2 {
        let message = tokio::time::timeout(Duration::from_secs(5), signals.next())
            .await
            .expect("a signal arrives")
            .expect("the stream is open")
            .expect("a valid message");
        let member = message.header().member().map(|member| member.to_string());
        match member.as_deref() {
            Some("ActivationToken") => {
                let (signal_id, token): (u32, String) = message.body().deserialize().expect("(us)");
                seen.push(format!("ActivationToken {signal_id} {token}"));
            }
            Some("ActionInvoked") => {
                let (signal_id, action): (u32, String) =
                    message.body().deserialize().expect("(us)");
                seen.push(format!("ActionInvoked {signal_id} {action}"));
            }
            _ => {}
        }
    }
    assert_eq!(
        seen,
        [
            format!("ActivationToken {id} token-1"),
            format!("ActionInvoked {id} default"),
        ]
    );

    service.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_post_and_a_notify_never_share_an_id() {
    let (bus, mut service, handle) = notifications().await;

    let client = bus.connection().await;
    let reply = client
        .call_method(
            Some(NAME),
            PATH,
            Some(NAME),
            "Notify",
            &(
                "Chat",
                0u32,
                "",
                "Marta",
                "Hello",
                Vec::<&str>::new(),
                HashMap::<&str, Value<'_>>::new(),
                -1i32,
            ),
        )
        .await
        .expect("notify answers");
    let notified: u32 = reply.body().deserialize().expect("an id");

    let posted = handle
        .post(flood("glimpse", "Posted"))
        .await
        .expect("post answers");

    assert_ne!(notified, posted);

    service.stop().await;
}

fn flood(app: &str, summary: &str) -> Incoming {
    Incoming {
        app_name: app.to_owned(),
        app_id: app.to_owned(),
        app_pid: None,
        icon: None,
        image: None,
        summary: summary.to_owned(),
        body: String::new(),
        actions: Vec::new(),
        urgency: Default::default(),
        progress: None,
        resident: false,
        expire_timeout: -1,
        transient: false,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unread_flood_evicts_the_oldest_non_pending_record_with_a_closed_signal() {
    let (bus, mut service, handle) = notifications().await;

    let client = bus.connection().await;
    let proxy = FreedesktopNotificationsProxy::new(&client)
        .await
        .expect("a proxy");
    let mut signals = proxy
        .receive_notification_closed()
        .await
        .expect("a signal stream");

    let pending = handle
        .post({
            let mut incoming = flood("Pinned", "stay");
            incoming.resident = true;
            incoming.expire_timeout = 0;
            incoming.actions = vec![("reply".to_owned(), "Reply".to_owned())];
            incoming
        })
        .await
        .expect("the pending record posts");
    let oldest = handle.post(flood("Flood", "one")).await.expect("posted");
    for n in 2..UNREAD_MAX {
        handle
            .post(flood("Flood", &n.to_string()))
            .await
            .expect("posted");
    }
    handle
        .subscribe()
        .wait_for(|state| {
            state
                .list
                .as_ref()
                .is_some_and(|list| list.notifications.len() == UNREAD_MAX)
        })
        .await
        .expect("the ceiling holds");
    let newest = handle.post(flood("Flood", "last")).await.expect("posted");

    let closed = tokio::time::timeout(Duration::from_secs(5), signals.next())
        .await
        .expect("a signal arrives")
        .expect("the stream is open");
    let args = closed.args().expect("closed arguments");
    assert_eq!(
        (*args.id(), *args.reason()),
        (oldest, 1),
        "the evicted id expires"
    );

    let ids: Vec<u32> = handle
        .snapshot()
        .list
        .expect("a list was published")
        .notifications
        .iter()
        .map(|record| record.id)
        .collect();
    assert_eq!(ids.len(), UNREAD_MAX);
    assert!(ids.contains(&pending), "a pending record survives the cap");
    assert!(
        !ids.contains(&oldest),
        "the oldest non-pending record is gone"
    );
    assert!(ids.contains(&newest), "the newest post is held");

    service.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn notify_image_data_pngs_follow_retention_suppression_and_eviction() {
    let name = "notify_image_data_pngs_follow_retention_suppression_and_eviction";
    if std::env::var_os("GLIMPSE_IMAGE_TEST").is_none() {
        let scratch = tempfile::tempdir().expect("a scratch directory");
        assert!(
            std::process::Command::new(std::env::current_exe().expect("test binary"))
                .args(["--exact", name])
                .env("GLIMPSE_IMAGE_TEST", name)
                .env("XDG_RUNTIME_DIR", scratch.path())
                .status()
                .expect("isolated test runs")
                .success()
        );
        return;
    }
    let (bus, mut service, handle) = notifications().await;
    let mut config = Config::default();
    config.notifications.suppress = vec!["^Suppressed$".to_owned()];
    service.reconfigure(&config);
    handle.clear_all().await.expect("configuration is applied");
    let client = bus.connection().await;
    let proxy = FreedesktopNotificationsProxy::new(&client)
        .await
        .expect("a proxy");
    let mut ids = Vec::new();
    for summary in ["Retained", "Suppressed"] {
        let pixels =
            zbus::zvariant::Structure::from((1i32, 1i32, 4i32, true, 8i32, 4i32, vec![255u8; 4]));
        ids.push(
            proxy
                .notify(
                    "Images",
                    0,
                    "",
                    summary,
                    "",
                    &[],
                    HashMap::from([("image-data", Value::from(pixels))]),
                    -1,
                )
                .await
                .expect("Notify(image-data) answers"),
        );
    }
    handle
        .post(flood("Barrier", "Processed"))
        .await
        .expect("Notify events are handled");
    let records = handle
        .snapshot()
        .list
        .expect("the list is published")
        .notifications;
    assert!(!records.iter().any(|record| record.id == ids[1]));
    let image = records
        .iter()
        .find(|record| record.id == ids[0])
        .and_then(|record| record.image.as_ref())
        .expect("the positive control retains its image");
    let reader = png::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(image).expect("the PNG exists"),
    ))
    .read_info()
    .expect("a real PNG");
    assert_eq!((reader.info().width, reader.info().height), (1, 1));
    let dir = std::path::Path::new(image)
        .parent()
        .expect("the images directory");
    wait_for_images(dir, 1).await;
    for _ in 0..UNREAD_MAX {
        handle.post(flood("Flood", "again")).await.expect("posted");
    }
    wait_for_images(dir, 0).await;
    service.stop().await;
}

async fn wait_for_images(dir: &std::path::Path, count: usize) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while std::fs::read_dir(dir).expect("generated images").count() != count {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("only retained notifications keep their PNGs");
}
