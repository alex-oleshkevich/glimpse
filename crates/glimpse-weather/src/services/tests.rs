use glimpse_services::ServiceState;

use super::*;

#[tokio::test]
async fn the_process_owns_weather_with_its_injected_location_service() {
    let services = WeatherServices::start_with_buses(
        &Config::default(),
        Buses::unavailable("no bus in tests"),
    );
    let weather = services.weather.health();

    services.shutdown().await;

    assert!(matches!(&*weather.borrow(), ServiceState::Stopped { .. }));
}
