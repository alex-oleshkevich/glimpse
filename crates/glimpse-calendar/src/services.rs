use anyhow::{Context as _, Result};
use glimpse_config::Config;
use glimpse_dbus::Buses;
use glimpse_services::{Calendar, CalendarHandle, Running};

use crate::provider;

pub struct CalendarServices {
    pub calendar: CalendarHandle,
    calendar_service: Running<Calendar>,
    provider: Option<provider::Runtime>,
}

impl CalendarServices {
    pub async fn start(document: &Config) -> Result<Self> {
        let session = zbus::Connection::session()
            .await
            .context("calendar provider needs the session bus")?;
        let mut services = Self::start_with_buses(document);
        match provider::start(session, services.calendar.clone()).await {
            Ok(provider) => services.provider = Some(provider),
            Err(error) => {
                services.shutdown().await;
                return Err(error).context("cannot start the calendar D-Bus provider");
            }
        }
        Ok(services)
    }

    fn start_with_buses(document: &Config) -> Self {
        let (calendar_service, calendar) = Running::<Calendar>::spawn(
            document,
            Buses::unavailable("calendar has no D-Bus backend"),
            (),
        );

        tracing::info!("calendar service graph started");
        Self {
            calendar,
            calendar_service,
            provider: None,
        }
    }

    pub fn reconfigure(&self, document: &Config) {
        self.calendar_service.reconfigure(document);
    }

    pub async fn shutdown(mut self) {
        tracing::info!("calendar provider shutting down");
        if let Some(provider) = self.provider.take() {
            provider.shutdown().await;
        }
        self.calendar_service.stop().await;
        tracing::info!("calendar service graph stopped");
    }
}

#[cfg(test)]
mod tests;
