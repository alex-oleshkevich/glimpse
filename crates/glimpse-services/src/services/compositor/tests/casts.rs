use super::*;

#[test]
fn casts_keep_privacy_active_until_the_last_stream_stops() {
    let mut state = Snapshot::default();

    apply(
        &mut state,
        Change::CastsChanged(vec![cast(3, true), cast(7, true)]),
    );
    apply(&mut state, Change::CastStopped(3));
    assert_eq!(
        state
            .casts
            .iter()
            .map(|cast| cast.stream_id)
            .collect::<Vec<_>>(),
        [7]
    );

    apply(
        &mut state,
        Change::CastStartedOrChanged {
            cast: cast(7, false),
        },
    );
    assert_eq!(state.casts.len(), 1);
    assert!(!state.casts[0].active);
}

#[tokio::test]
async fn a_snapshot_publishes_privacy_as_active_when_any_cast_is() {
    let mut harness = harness(Backend::Unsupported);

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Snapshot(Box::new(Snapshot {
                casts: vec![cast(2, false), cast(7, true)],
                ..Snapshot::default()
            }))),
        )
        .await;

    let published = harness
        .state
        .borrow()
        .privacy
        .clone()
        .expect("privacy published");
    assert!(published.active);
    assert_eq!(published.casts.len(), 2);
}

#[tokio::test]
async fn a_paused_cast_still_publishes_privacy_as_active() {
    let mut harness = harness(Backend::Unsupported);

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Snapshot(Box::new(Snapshot {
                casts: vec![cast(2, false), cast(7, false)],
                ..Snapshot::default()
            }))),
        )
        .await;

    let published = harness
        .state
        .borrow()
        .privacy
        .clone()
        .expect("privacy published");
    assert!(
        published.active,
        "OBS pauses every stream on a scene switch without ending the session, so a session \
             with no currently-streaming cast is still one a portal will not prompt for again"
    );
}

#[tokio::test]
async fn no_casts_publishes_privacy_as_inactive() {
    let mut harness = harness(Backend::Unsupported);

    harness
        .service
        .handle(&harness.ctx, Input::Event(Event::Snapshot(Box::default())))
        .await;

    let published = harness
        .state
        .borrow()
        .privacy
        .clone()
        .expect("privacy published");
    assert!(!published.active);
    assert!(published.casts.is_empty());
}
