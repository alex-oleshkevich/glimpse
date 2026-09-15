use std::fmt;

use anyhow::{Context as _, Result};
use glimpse_config::Config;
use glimpse_dbus::Buses;
use glimpse_services::{Geolocation, Running, Weather, WeatherDependencies, WeatherHandle};

use crate::provider;

pub struct WeatherServices {
    pub weather: WeatherHandle,
    weather_service: Running<Weather>,
    location_service: Running<Geolocation>,
    provider: Option<provider::Runtime>,
}

impl WeatherServices {
    pub async fn start(document: &Config) -> Result<Self> {
        let buses = Buses::connect().await;
        let session = buses
            .session_bus()
            .cloned()
            .map_err(|reason| anyhow::anyhow!(reason.to_owned()))
            .context("weather provider needs the session bus")?;
        let mut services = Self::start_with_buses(document, buses);
        match provider::start(session, services.weather.clone()).await {
            Ok(provider) => services.provider = Some(provider),
            Err(error) => {
                services.shutdown().await;
                return Err(error).context("cannot start the weather D-Bus provider");
            }
        }
        Ok(services)
    }

    fn start_with_buses(document: &Config, buses: Buses) -> Self {
        let (location_service, location) = Running::spawn(document, buses.clone(), ());
        let (weather_service, weather) = Running::spawn(
            document,
            buses,
            WeatherDependencies {
                geolocation: location,
            },
        );

        tracing::info!("weather service graph started");
        Self {
            weather,
            weather_service,
            location_service,
            provider: None,
        }
    }

    pub fn reconfigure(&self, document: &Config) {
        self.location_service.reconfigure(document);
        self.weather_service.reconfigure(document);
    }

    pub async fn shutdown(mut self) {
        self.cancel();
        tracing::info!("weather provider shutting down");
        if let Some(provider) = self.provider.take() {
            provider.shutdown().await;
        }
        self.weather_service.stop().await;
        self.location_service.stop().await;
        tracing::info!("weather service graph stopped");
    }

    fn cancel(&self) {
        self.weather_service.cancel();
        self.location_service.cancel();
    }
}

impl fmt::Debug for WeatherServices {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WeatherServices")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
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
}
