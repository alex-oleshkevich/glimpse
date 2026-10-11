use std::collections::HashMap;
use std::time::Duration;

use futures_util::StreamExt;
use glimpse_config::Config;
use glimpse_dbus::Buses;
use glimpse_dbus::testing::PrivateBus;
use glimpse_services::{Incoming, Notifications, Running, ServiceState};
use zbus::zvariant::Value;
use zbus::{MatchRule, MessageStream, message::Type};

const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn activation_hands_over_the_token_before_reporting_the_default_action() {
    let bus = PrivateBus::start();
    let (mut service, handle) = Running::<Notifications>::spawn(
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
    let bus = PrivateBus::start();
    let (mut service, handle) = Running::<Notifications>::spawn(
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
        .post(Incoming {
            app_name: "glimpse".to_owned(),
            app_id: "glimpse".to_owned(),
            app_pid: None,
            icon: None,
            image: None,
            summary: "Posted".to_owned(),
            body: String::new(),
            actions: Vec::new(),
            urgency: Default::default(),
            progress: None,
            resident: false,
            expire_timeout: -1,
            transient: false,
        })
        .await
        .expect("post answers");

    assert_ne!(notified, posted);

    service.stop().await;
}
