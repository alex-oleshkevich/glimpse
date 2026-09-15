use std::fmt;

use anyhow::{Context as _, Result};
use glimpse_config::Config;
use glimpse_dbus::Buses;
use glimpse_services::{
    Geolocation, Service, ServiceRuntime, ServiceSender, Weather, WeatherDependencies,
    WeatherHandle,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::provider;

pub struct WeatherServices {
    pub weather: WeatherHandle,
    weather_sender: ServiceSender<Weather>,
    weather_cancel: CancellationToken,
    weather_task: Option<JoinHandle<()>>,
    location_sender: ServiceSender<Geolocation>,
    location_cancel: CancellationToken,
    location_task: Option<JoinHandle<()>>,
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
        let location_cancel = CancellationToken::new();
        let (location_runtime, location) = ServiceRuntime::<Geolocation>::new(
            <Geolocation as Service>::Config::from(document),
            buses.clone(),
            location_cancel.clone(),
        );
        let location_sender = location_runtime.sender();
        let location_task = spawn_service(location_runtime, ());

        let weather_cancel = CancellationToken::new();
        let (weather_runtime, weather) = ServiceRuntime::<Weather>::new(
            <Weather as Service>::Config::from(document),
            buses,
            weather_cancel.clone(),
        );
        let weather_sender = weather_runtime.sender();
        let weather_task = spawn_service(
            weather_runtime,
            WeatherDependencies {
                geolocation: location,
            },
        );

        tracing::info!("weather service graph started");
        Self {
            weather,
            weather_sender,
            weather_cancel,
            weather_task: Some(weather_task),
            location_sender,
            location_cancel,
            location_task: Some(location_task),
            provider: None,
        }
    }

    pub fn reconfigure(&self, document: &Config) {
        self.location_sender
            .reconfigure(<Geolocation as Service>::Config::from(document));
        self.weather_sender
            .reconfigure(<Weather as Service>::Config::from(document));
    }

    pub async fn shutdown(mut self) {
        tracing::info!("weather provider shutting down");
        if let Some(provider) = self.provider.take() {
            provider.shutdown().await;
        }
        stop(Weather::NAME, &self.weather_cancel, &mut self.weather_task).await;
        stop(
            Geolocation::NAME,
            &self.location_cancel,
            &mut self.location_task,
        )
        .await;
        tracing::info!("weather service graph stopped");
    }

    fn cancel(&self) {
        if let Some(provider) = &self.provider {
            provider.cancel();
        }
        self.weather_cancel.cancel();
        self.location_cancel.cancel();
    }
}

impl Drop for WeatherServices {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl fmt::Debug for WeatherServices {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WeatherServices")
            .finish_non_exhaustive()
    }
}

fn spawn_service<S: Service>(
    mut runtime: ServiceRuntime<S>,
    dependencies: S::Dependencies,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        tracing::debug!(service = S::NAME, "service task starting");
        if let Err(error) = runtime.run(dependencies).await {
            tracing::error!(service = S::NAME, %error, "service stopped");
        } else {
            tracing::debug!(service = S::NAME, "service task stopped");
        }
    })
}

async fn stop(
    service: &'static str,
    cancel: &CancellationToken,
    task: &mut Option<JoinHandle<()>>,
) {
    cancel.cancel();
    if let Some(task) = task.take()
        && let Err(error) = task.await
    {
        tracing::error!(service, %error, "service task failed");
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
