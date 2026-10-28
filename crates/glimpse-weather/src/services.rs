use anyhow::{Context as _, Result};
use glimpse_config::Config;
use glimpse_dbus::Buses;
use glimpse_dbus::settings::SettingsProvider;
use glimpse_services::{Running, Weather, WeatherDependencies, WeatherHandle};

use crate::provider;

pub struct WeatherServices {
    pub weather: WeatherHandle,
    weather_service: Running<Weather>,
    location: SettingsProvider,
    provider: Option<provider::Runtime>,
}

impl WeatherServices {
    pub async fn start(document: &Config) -> Result<Self> {
        let session = zbus::Connection::session()
            .await
            .context("weather provider needs the session bus")?;
        let mut services = Self::start_with_buses(document, session.clone());
        match provider::start(session, services.weather.clone()).await {
            Ok(provider) => services.provider = Some(provider),
            Err(error) => {
                services.shutdown().await;
                return Err(error).context("cannot start the weather D-Bus provider");
            }
        }
        Ok(services)
    }

    fn start_with_buses(document: &Config, session: zbus::Connection) -> Self {
        let location = SettingsProvider::start(session);
        let (weather_service, weather) = Running::spawn(
            document,
            Buses::unavailable("weather has no D-Bus backend"),
            WeatherDependencies {
                geolocation: location.subscribe_location(),
            },
        );

        tracing::info!("weather service graph started");
        Self {
            weather,
            weather_service,
            location,
            provider: None,
        }
    }

    pub fn reconfigure(&self, document: &Config) {
        self.weather_service.reconfigure(document);
    }

    pub fn bus_closed(&self) -> impl Future<Output = ()> + Send + 'static {
        let closed = self.provider.as_ref().map(provider::Runtime::closed);
        async move {
            match closed {
                Some(closed) => closed.await,
                None => std::future::pending().await,
            }
        }
    }

    pub async fn shutdown(mut self) {
        tracing::info!("weather provider shutting down");
        if let Some(provider) = self.provider.take() {
            provider.shutdown().await;
        }
        self.weather_service.stop().await;
        self.location.shutdown().await;
        tracing::info!("weather service graph stopped");
    }
}

#[cfg(test)]
mod tests;
