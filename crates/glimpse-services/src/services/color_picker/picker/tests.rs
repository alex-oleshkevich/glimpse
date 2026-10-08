use super::*;

#[tokio::test]
async fn an_unavailable_session_bus_is_reported() {
    let picker = PortalPicker::new(Buses::unavailable("no session bus"));
    assert_eq!(picker.pick().await, Err("no session bus".to_owned()));
}
