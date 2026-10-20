use chrono::TimeZone as _;

use super::*;

#[test]
fn a_loop_status_survives_a_round_trip_through_the_wire_name() {
    for value in [Repeat::Off, Repeat::Playlist, Repeat::Track] {
        assert_eq!(repeat(status(value)), value);
    }
    assert_eq!(
        status(Repeat::Unknown),
        "None",
        "a status this daemon does not know must not be sent back as itself"
    );
}

#[test]
fn a_playback_status_the_spec_does_not_name_is_unknown_rather_than_stopped() {
    assert_eq!(playback(Some("Playing")), Playback::Playing);
    assert_eq!(playback(Some("Paused")), Playback::Paused);
    assert_eq!(playback(Some("Stopped")), Playback::Stopped);
    assert_eq!(playback(Some("Buffering")), Playback::Unknown);
    assert_eq!(playback(None), Playback::Unknown);
}

#[test]
fn a_re_read_that_changes_nothing_does_not_count_as_activity() {
    let earlier = Utc.with_ymd_and_hms(2026, 9, 9, 12, 0, 0).unwrap();
    let later = Utc.with_ymd_and_hms(2026, 9, 9, 12, 30, 0).unwrap();

    let mut held = select::tests::sample();
    held.playback = Playback::Playing;
    held.last_active = earlier;

    let mut fresh = held.clone();
    fresh.last_active = later;
    assert_eq!(
        stirred(Some(&held), &fresh),
        earlier,
        "an unchanged playback state is the same activity, however many times it is re-read"
    );

    fresh.playback = Playback::Paused;
    assert_eq!(stirred(Some(&held), &fresh), later);
    assert_eq!(
        stirred(None, &fresh),
        later,
        "a player just seen is active now"
    );
}

#[test]
fn art_max_kib_is_clamped_where_the_document_meets_the_service() {
    let mut document = glimpse_config::Config::default();
    document.mpris.art_max_kib = 0;
    assert_eq!(Config::from(&document).art_max_kib, 16);

    document.mpris.art_max_kib = u32::MAX;
    assert_eq!(Config::from(&document).art_max_kib, 65_536);
}

#[tokio::test]
async fn art_files_the_service_no_longer_references_are_deleted() {
    use crate::ServiceState;
    use tokio::sync::{mpsc, watch};
    use tokio_util::sync::CancellationToken;

    let runtime = tempfile::tempdir().expect("tempdir").keep();
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", &runtime) };
    let (first, second) = ("https://example.test/a.png", "https://example.test/b.png");
    let held = art::cached(first).expect("a cache path");
    std::fs::create_dir_all(held.parent().expect("a cache directory")).expect("cache directory");
    std::fs::write(&held, b"cover").expect("cached cover");

    let (events, _inbox) = mpsc::channel(8);
    let cancel = CancellationToken::new();
    let (health, _health) = watch::channel(ServiceState::Starting);
    let (published, _state) = watch::channel(MprisPlayers::default());
    let ctx = Ctx::<Mpris>::new(
        events,
        &cancel,
        published,
        health,
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );
    let mut service =
        Mpris::start(&ctx, Config::from(&glimpse_config::Config::default()), ()).await;

    let mut player = select::tests::sample();
    player.art_url = Some(first.to_owned());
    let bus = player.bus.clone();
    service
        .handle(&ctx, Input::Event(Event::Appeared(bus)))
        .await;
    service
        .handle(&ctx, Input::Event(Event::Updated(Box::new(player.clone()))))
        .await;
    service
        .handle(
            &ctx,
            Input::Event(Event::Art {
                url: first.to_owned(),
                path: Some(held.display().to_string()),
            }),
        )
        .await;
    assert!(held.exists());

    player.art_url = Some(second.to_owned());
    service
        .handle(&ctx, Input::Event(Event::Updated(Box::new(player))))
        .await;

    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while held.exists() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the unreferenced cover is deleted");

    art::forget(first.to_owned()).await;
}
