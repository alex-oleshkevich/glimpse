use std::fmt;

use glimpse_config::Config;
use glimpse_dbus::Buses;
use glimpse_services::{
    Compositor, CompositorHandle, Notifications, NotificationsHandle, Service, ServiceRuntime,
    ServiceSender, Session, SessionDependencies, SessionHandle, initial_compositor_state,
    initial_notifications_state, initial_session_state,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::provider;

pub struct NotificationServices {
    pub notifications: NotificationsHandle,
    pub compositor: CompositorHandle,
    pub session: SessionHandle,
    notifications_sender: ServiceSender<Notifications>,
    compositor_sender: ServiceSender<Compositor>,
    session_sender: ServiceSender<Session>,
    notifications_cancel: CancellationToken,
    compositor_cancel: CancellationToken,
    session_cancel: CancellationToken,
    notifications_task: Option<JoinHandle<()>>,
    compositor_task: Option<JoinHandle<()>>,
    session_task: Option<JoinHandle<()>>,
    provider: Option<provider::Runtime>,
}

impl NotificationServices {
    pub async fn start(document: &Config) -> Self {
        let buses = Buses::connect().await;
        let session = buses.session_bus().ok().cloned();
        let mut services = Self::start_with_buses(document, buses);
        if let Some(connection) = session {
            match provider::Runtime::start(connection, services.notifications.clone()).await {
                Ok(provider) => services.provider = Some(provider),
                Err(error) => tracing::warn!(%error, "notification provider unavailable"),
            }
        }
        services
    }

    fn start_with_buses(document: &Config, buses: Buses) -> Self {
        let compositor_cancel = CancellationToken::new();
        let (compositor_runtime, compositor) = ServiceRuntime::<Compositor>::new(
            initial_compositor_state(),
            buses.clone(),
            compositor_cancel.clone(),
        );
        let compositor_sender = compositor_runtime.sender();
        let compositor_task = spawn_service(document, compositor_runtime, ());

        let session_cancel = CancellationToken::new();
        let (session_runtime, session) = ServiceRuntime::<Session>::new(
            initial_session_state(),
            buses.clone(),
            session_cancel.clone(),
        );
        let session_sender = session_runtime.sender();
        let session_task = spawn_service(
            document,
            session_runtime,
            SessionDependencies {
                compositor: compositor.clone(),
            },
        );

        let notifications_cancel = CancellationToken::new();
        let (notifications_runtime, notifications) = ServiceRuntime::<Notifications>::new(
            initial_notifications_state(),
            buses,
            notifications_cancel.clone(),
        );
        let notifications_sender = notifications_runtime.sender();
        let notifications_task = spawn_service(document, notifications_runtime, ());

        Self {
            notifications,
            compositor,
            session,
            notifications_sender,
            compositor_sender,
            session_sender,
            notifications_cancel,
            compositor_cancel,
            session_cancel,
            notifications_task: Some(notifications_task),
            compositor_task: Some(compositor_task),
            session_task: Some(session_task),
            provider: None,
        }
    }

    pub fn reconfigure(&self, document: &Config) {
        self.notifications_sender
            .reconfigure(<Notifications as Service>::Config::from(document));
        self.session_sender
            .reconfigure(<Session as Service>::Config::from(document));
        self.compositor_sender
            .reconfigure(<Compositor as Service>::Config::from(document));
    }

    pub async fn shutdown(mut self) {
        stop(
            Notifications::NAME,
            &self.notifications_cancel,
            &mut self.notifications_task,
        )
        .await;
        if let Some(provider) = self.provider.take() {
            provider.shutdown().await;
        }
        stop(Session::NAME, &self.session_cancel, &mut self.session_task).await;
        stop(
            Compositor::NAME,
            &self.compositor_cancel,
            &mut self.compositor_task,
        )
        .await;
    }

    fn cancel(&self) {
        if let Some(provider) = &self.provider {
            provider.cancel();
        }
        self.notifications_cancel.cancel();
        self.session_cancel.cancel();
        self.compositor_cancel.cancel();
    }
}

impl Drop for NotificationServices {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl fmt::Debug for NotificationServices {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NotificationServices")
            .finish_non_exhaustive()
    }
}

fn spawn_service<S: Service>(
    document: &Config,
    mut runtime: ServiceRuntime<S>,
    dependencies: S::Dependencies,
) -> JoinHandle<()> {
    let config = S::Config::from(document);
    tokio::spawn(async move {
        if let Err(error) = runtime.run(config, dependencies).await {
            tracing::error!(service = S::NAME, %error, "service stopped");
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
