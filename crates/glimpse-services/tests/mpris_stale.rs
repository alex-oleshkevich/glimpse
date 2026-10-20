use std::collections::HashMap;
use std::time::Duration;

use glimpse_config::Config;
use glimpse_dbus::Buses;
use glimpse_dbus::testing::PrivateBus;
use glimpse_services::{Mpris, Running};
use zbus::zvariant::{OwnedValue, Value};

struct Root;

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    #[zbus(property)]
    fn identity(&self) -> String {
        "Fake".to_owned()
    }
}

struct Player {
    title: String,
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        HashMap::from([(
            "xesam:title".to_owned(),
            OwnedValue::try_from(Value::from(self.title.as_str())).unwrap(),
        )])
    }

    #[zbus(property)]
    fn playback_status(&self) -> String {
        "Playing".to_owned()
    }

    #[zbus(property)]
    fn position(&self) -> i64 {
        0
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_published_title_follows_the_last_track_change() {
    let bus = PrivateBus::start();
    let player_connection = bus.connection().await;
    player_connection
        .object_server()
        .at("/org/mpris/MediaPlayer2", Root)
        .await
        .unwrap();
    player_connection
        .object_server()
        .at(
            "/org/mpris/MediaPlayer2",
            Player {
                title: "track-0".to_owned(),
            },
        )
        .await
        .unwrap();
    glimpse_dbus::own_name(&player_connection, "org.mpris.MediaPlayer2.fake")
        .await
        .unwrap();

    let service_connection = bus.connection().await;
    let (_service, handle) = Running::<Mpris>::spawn(
        &Config::default(),
        Buses::for_session(service_connection),
        (),
    );
    let mut players = handle.subscribe();

    let wait_for = |players: &mut tokio::sync::watch::Receiver<glimpse_services::MprisPlayers>,
                    want: String| {
        let mut players = players.clone();
        async move {
            tokio::time::timeout(Duration::from_secs(2), async move {
                loop {
                    if players
                        .borrow_and_update()
                        .players
                        .first()
                        .and_then(|p| p.title.clone())
                        .as_deref()
                        == Some(want.as_str())
                    {
                        return;
                    }
                    players.changed().await.unwrap();
                }
            })
            .await
            .is_ok()
        }
    };
    assert!(wait_for(&mut players, "track-0".to_owned()).await);

    let iface = player_connection
        .object_server()
        .interface::<_, Player>("/org/mpris/MediaPlayer2")
        .await
        .unwrap();
    let mut stale = Vec::new();
    for n in 1..=200 {
        let title = format!("track-{n}");
        iface.get_mut().await.title = title.clone();
        iface
            .get()
            .await
            .metadata_changed(iface.signal_emitter())
            .await
            .unwrap();
        if !wait_for(&mut players, title.clone()).await {
            let shown = handle
                .snapshot()
                .players
                .first()
                .and_then(|p| p.title.clone());
            stale.push((title, shown));
        }
    }
    assert!(
        stale.is_empty(),
        "{} of 200 track changes left a stale title: {:?}",
        stale.len(),
        &stale[..stale.len().min(5)]
    );
}
