use glimpse_dbus::testing::PrivateBus;
use glimpse_services::ServiceState;

use super::*;

#[tokio::test]
async fn the_process_owns_weather_with_its_settings_follower() {
    let bus = PrivateBus::start();
    let connection = bus.connection().await;
    let services = WeatherServices::start_with_buses(
        &Config::default(),
        Buses::unavailable("no bus in tests"),
        connection,
    );
    let weather = services.weather.health();

    services.shutdown().await;

    assert!(matches!(&*weather.borrow(), ServiceState::Stopped { .. }));
}
