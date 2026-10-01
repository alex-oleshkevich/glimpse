use super::*;

#[tokio::test]
async fn the_location_source_reports_unavailable_without_a_system_bus() {
    let cancel = tokio_util::sync::CancellationToken::new();
    let (events, _inbox) = tokio::sync::mpsc::channel::<crate::service::Input<Privacy>>(8);
    let (state, _state_rx) =
        tokio::sync::watch::channel(super::super::super::super::PrivacyState::default());
    let (health, _health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
    let ctx = Ctx::<Privacy>::new(
        events,
        &cancel,
        state,
        health,
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );

    let mut stream = location(ctx).await;

    let event = futures_util::FutureExt::now_or_never(stream.next())
        .expect("a stream::once with no real await point resolves synchronously")
        .expect("one item");
    assert!(
        matches!(event, Event::Unavailable(Resource::Location, _)),
        "with no system bus the source must say it cannot see, never silently report nothing \
             in use"
    );
}
