use super::*;

#[test]
fn an_unavailable_bus_reports_why() {
    let buses = Buses::unavailable("no bus in tests");
    assert_eq!(buses.session_bus().unwrap_err(), "no bus in tests");
    assert_eq!(buses.system_bus().unwrap_err(), "no bus in tests");
}
