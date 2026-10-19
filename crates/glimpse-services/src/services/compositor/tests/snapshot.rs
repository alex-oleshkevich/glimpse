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
