use super::*;

#[tokio::test]
async fn a_status_is_published_before_any_snapshot_arrives() {
    let harness = harness(Backend::Unsupported);

    let published = harness.state.borrow().clone();
    assert_eq!(
        published.status.map(|status| status.name),
        Some(Backend::Unsupported.name().to_owned()),
        "the bar needs a compositor name before the first snapshot resolves"
    );
    assert!(
        published.workspaces.is_none(),
        "nothing is known about workspaces until a snapshot arrives"
    );
}

#[tokio::test]
async fn a_change_before_the_first_snapshot_publishes_nothing() {
    let mut harness = harness(Backend::Unsupported);

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Changed(Change::WorkspacesChanged(Vec::new()))),
        )
        .await;

    assert!(
        harness.state.borrow().workspaces.is_none(),
        "a change with no snapshot behind it is dropped rather than published as empty"
    );
}

#[tokio::test]
async fn the_first_snapshot_publishes_and_clears_a_degraded_health() {
    let mut harness = harness(Backend::Unsupported);
    harness.ctx.degraded("no compositor yet".to_owned());

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Snapshot(Box::new(snapshot_of(
                vec![workspace(1, Some(1), "DP-2")],
                Vec::new(),
            )))),
        )
        .await;

    assert!(!harness.ctx.is_degraded(), "a snapshot clears degraded");
    let published = harness.state.borrow().clone();
    assert_eq!(
        published.workspaces.map(|shown| shown.workspaces.len()),
        Some(1)
    );
}

fn fetch_attempt(service: &Compositor) -> u64 {
    service
        .subscriptions()
        .iter()
        .find_map(|sub| match sub.key() {
            Watch::Fetch { attempt } => Some(*attempt),
            Watch::Events => None,
        })
        .expect("a fetch is always declared")
}

#[tokio::test]
async fn a_failed_first_snapshot_is_fetched_again_when_a_change_arrives() {
    let mut harness = harness(Backend::Unsupported);
    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::FetchFailed("first snapshot failed".to_owned())),
        )
        .await;
    let before = fetch_attempt(&harness.service);

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Changed(Change::Resync(Resync::Structure))),
        )
        .await;

    assert_ne!(
        fetch_attempt(&harness.service),
        before,
        "an unchanged fetch key leaves the failed snapshot dead for the whole session"
    );
}

#[tokio::test]
async fn changes_while_the_first_fetch_is_in_flight_do_not_restart_it() {
    let mut harness = harness(Backend::Unsupported);
    let before = fetch_attempt(&harness.service);

    for _ in 0..5 {
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Changed(Change::Resync(Resync::Structure))),
            )
            .await;
    }

    assert_eq!(
        fetch_attempt(&harness.service),
        before,
        "every restart cancels the fetch that is still running, so a steady event flow starves it"
    );
}

#[tokio::test]
async fn a_change_before_the_first_snapshot_refetches_once_when_it_lands() {
    let mut harness = harness(Backend::Unsupported);
    let before = harness.service.attempt;

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Changed(Change::WorkspacesChanged(Vec::new()))),
        )
        .await;
    assert_eq!(
        harness.service.attempt, before,
        "nothing refetches before the snapshot lands"
    );

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Snapshot(Box::new(snapshot_of(
                Vec::new(),
                Vec::new(),
            )))),
        )
        .await;
    assert_eq!(harness.service.attempt, before + 1);

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Snapshot(Box::new(snapshot_of(
                Vec::new(),
                Vec::new(),
            )))),
        )
        .await;
    assert_eq!(
        harness.service.attempt,
        before + 1,
        "a second snapshot does not refetch again"
    );
}

#[tokio::test]
async fn a_snapshot_with_no_change_during_the_fetch_does_not_refetch() {
    let mut harness = harness(Backend::Unsupported);
    let before = harness.service.attempt;

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Snapshot(Box::new(snapshot_of(
                Vec::new(),
                Vec::new(),
            )))),
        )
        .await;

    assert_eq!(harness.service.attempt, before);
}
