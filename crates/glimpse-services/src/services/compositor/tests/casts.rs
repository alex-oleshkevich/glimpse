use super::*;

fn ids(state: &CompositorState) -> Vec<u64> {
    state
        .privacy
        .as_ref()
        .map(|privacy| privacy.casts.iter().map(|cast| cast.stream_id).collect())
        .unwrap_or_default()
}

#[tokio::test]
async fn casts_keep_privacy_active_until_the_last_stream_stops() {
    let mut rig = Rig::start(Snapshot::default()).await;

    rig.send(BackendEvent::CastsChanged(vec![
        cast(3, true),
        cast(7, true),
    ]));
    rig.until("two casts", |state| ids(state) == [3, 7]).await;
    rig.send(BackendEvent::CastStopped(3));
    rig.until("one cast", |state| ids(state) == [7]).await;

    rig.send(BackendEvent::CastStartedOrChanged(cast(7, false)));
    rig.until("a paused cast", |state| {
        state
            .privacy
            .as_ref()
            .is_some_and(|privacy| privacy.casts.len() == 1 && !privacy.casts[0].active)
    })
    .await;
}

#[tokio::test]
async fn a_snapshot_publishes_privacy_as_active_when_any_cast_is() {
    let rig = Rig::start(Snapshot {
        casts: vec![cast(2, false), cast(7, true)],
        ..Snapshot::default()
    })
    .await;

    let published = rig
        .mirror
        .handle
        .snapshot()
        .privacy
        .expect("privacy published");
    assert!(published.active);
    assert_eq!(published.casts.len(), 2);
}

#[tokio::test]
async fn a_paused_cast_still_publishes_privacy_as_active() {
    let rig = Rig::start(Snapshot {
        casts: vec![cast(2, false), cast(7, false)],
        ..Snapshot::default()
    })
    .await;

    let published = rig
        .mirror
        .handle
        .snapshot()
        .privacy
        .expect("privacy published");
    assert!(
        published.active,
        "OBS pauses every stream on a scene switch without ending the session, so a session \
             with no currently-streaming cast is still one a portal will not prompt for again"
    );
}

#[tokio::test]
async fn no_casts_publishes_privacy_as_inactive() {
    let rig = Rig::start(Snapshot::default()).await;

    let published = rig
        .mirror
        .handle
        .snapshot()
        .privacy
        .expect("privacy published");
    assert!(!published.active);
    assert!(published.casts.is_empty());
}
