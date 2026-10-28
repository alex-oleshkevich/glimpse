use anyhow::{Context as _, Result};
use glimpse_config::Config;
use glimpse_dbus::Buses;
use glimpse_services::{Calendar, CalendarHandle, Running};
use tokio::task::JoinHandle;

use crate::{notify, provider};

pub struct CalendarServices {
    pub calendar: CalendarHandle,
    calendar_service: Running<Calendar>,
    provider: Option<provider::Runtime>,
    notifier: Option<JoinHandle<()>>,
}

impl CalendarServices {
    pub async fn start(document: &Config) -> Result<Self> {
        let session = zbus::Connection::session()
            .await
            .context("calendar provider needs the session bus")?;
        let mut services = Self::start_with_buses(document);
        match provider::start(session.clone(), services.calendar.clone()).await {
            Ok(provider) => services.provider = Some(provider),
            Err(error) => {
                services.shutdown().await;
                return Err(error).context("cannot start the calendar D-Bus provider");
            }
        }
        services.notifier = Some(tokio::spawn(notify::follow(
            session,
            services.calendar.subscribe(),
        )));
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
            notifier: None,
        }
    }

    pub fn reconfigure(&self, document: &Config) {
        self.calendar_service.reconfigure(document);
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
        tracing::info!("calendar provider shutting down");
        if let Some(notifier) = self.notifier.take() {
            notifier.abort();
        }
        if let Some(provider) = self.provider.take() {
            provider.shutdown().await;
        }
        self.calendar_service.stop().await;
        tracing::info!("calendar service graph stopped");
    }
}

#[cfg(test)]
mod tests;
