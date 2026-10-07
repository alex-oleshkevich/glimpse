use super::*;

#[tokio::test]
async fn without_a_session_bus_the_pick_fails_with_the_reason() {
    let picker = PortalPicker::new(Buses::unavailable("x"));

    assert_eq!(picker.pick().await, Err("x".to_owned()));
}
