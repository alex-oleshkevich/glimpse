use glimpse_services::ServiceState;

use super::*;

#[tokio::test]
async fn the_process_stops_its_calendar_service() {
    let services = CalendarServices::start_with_buses(&Config::default());
    let health = services.calendar.health();
    services.shutdown().await;
    assert!(matches!(&*health.borrow(), ServiceState::Stopped { .. }));
}
