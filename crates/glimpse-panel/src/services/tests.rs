use glimpse_services::ServiceState;

use super::*;

#[tokio::test]
async fn the_panel_owns_one_typed_local_service_graph() {
    let services =
        PanelServices::start_with_buses(&Config::default(), Buses::unavailable("no bus in tests"));
    let health = [
        services.handles.compositor.health(),
        services.handles.keyboard.health(),
        services.handles.calendar.health(),
        services.handles.mpris.health(),
        services.exec().health(),
        services.handles.heartbeat.health(),
        services.handles.tray.health(),
    ];

    services.shutdown().await;

    for state in health {
        assert!(matches!(&*state.borrow(), ServiceState::Stopped { .. }));
    }
}
