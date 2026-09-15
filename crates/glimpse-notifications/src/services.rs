use std::fmt;

use anyhow::{Context as _, Result};

use glimpse_config::Config;
use glimpse_dbus::Buses;
use glimpse_services::{
    Compositor, CompositorHandle, Notifications, NotificationsHandle, Running, Session,
    SessionDependencies, SessionHandle,
};

use crate::provider;

pub struct NotificationServices {
    pub notifications: NotificationsHandle,
    pub compositor: CompositorHandle,
    pub session: SessionHandle,
    notifications_service: Running<Notifications>,
    compositor_service: Running<Compositor>,
    session_service: Running<Session>,
    provider: Option<provider::Runtime>,
}

impl NotificationServices {
    pub async fn start(document: &Config) -> Result<Self> {
        let buses = Buses::connect().await;
        let session = buses
            .session_bus()
            .cloned()
            .map_err(|reason| anyhow::anyhow!(reason.to_owned()))
            .context("the notification provider needs the session bus")?;
        let mut services = Self::start_with_buses(document, buses);
        match provider::start(session, services.notifications.clone()).await {
            Ok(provider) => services.provider = Some(provider),
            Err(error) => {
                services.shutdown().await;
                return Err(error).context("cannot start the notification D-Bus provider");
            }
        }
        Ok(services)
    }

    fn start_with_buses(document: &Config, buses: Buses) -> Self {
        let (compositor_service, compositor) =
            Running::<Compositor>::spawn(document, buses.clone(), ());
        let (session_service, session) = Running::spawn(
            document,
            buses.clone(),
            SessionDependencies {
                compositor: compositor.clone(),
            },
        );
        let (notifications_service, notifications) =
            Running::<Notifications>::spawn(document, buses, ());

        Self {
            notifications,
            compositor,
            session,
            notifications_service,
            compositor_service,
            session_service,
            provider: None,
        }
    }

    pub fn reconfigure(&self, document: &Config) {
        self.notifications_service.reconfigure(document);
        self.session_service.reconfigure(document);
        self.compositor_service.reconfigure(document);
    }

    pub async fn shutdown(mut self) {
        self.cancel();
        self.notifications_service.stop().await;
        if let Some(provider) = self.provider.take() {
            provider.shutdown().await;
        }
        self.session_service.stop().await;
        self.compositor_service.stop().await;
    }

    fn cancel(&self) {
        self.notifications_service.cancel();
        self.session_service.cancel();
        self.compositor_service.cancel();
    }
}

impl fmt::Debug for NotificationServices {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NotificationServices")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
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
}
