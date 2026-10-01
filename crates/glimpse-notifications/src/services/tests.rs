use glimpse_services::ServiceState;

use super::*;

#[tokio::test]
async fn the_process_owns_one_typed_local_service_graph() {
    let services = NotificationServices::start_with_buses(
        &Config::default(),
        Buses::unavailable("no bus in tests"),
    );
    let health = [
        services.notifications.health(),
        services.session.health(),
        services.compositor.health(),
    ];

    services.shutdown().await;

    for state in health {
        assert!(matches!(&*state.borrow(), ServiceState::Stopped { .. }));
    }
}
