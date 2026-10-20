use super::*;

#[tokio::test]
async fn review_bluetoothd_leaving_the_bus_does_not_keep_a_connected_headset() {
    let (mut service, ctx, state, _health) = bluetooth().await;
    enumerated(&mut service, &ctx, session()).await;
    assert!(state.borrow().connected().next().is_some());

    service
        .handle(&ctx, Input::Event(Event::NameOwner(None)))
        .await;

    let held = state.borrow().clone();
    assert!(
        held.connected().next().is_none() && held.adapter.is_none(),
        "bluetoothd is gone, yet the state still shows {:?} with {:?}",
        held.adapter,
        held.devices
            .iter()
            .filter(|d| d.connected)
            .map(|d| &d.name)
            .collect::<Vec<_>>()
    );
}
