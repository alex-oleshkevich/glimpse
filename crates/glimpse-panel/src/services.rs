use std::fmt;

use glimpse_config::Config;
use glimpse_contracts::KeyboardLayouts;
use glimpse_dbus::{Buses, notifications::NotificationsProviderHandle};
use glimpse_services::{
    Calendar, CalendarHandle, Compositor, CompositorHandle, Heartbeat, HeartbeatHandle, Keyboard,
    KeyboardDependencies, KeyboardHandle, Mpris, MprisHandle, Service, ServiceRuntime,
    ServiceSender, initial_calendar_state, initial_compositor_state, initial_mpris_state,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub struct PanelServices {
    pub compositor: CompositorHandle,
    compositor_sender: ServiceSender<Compositor>,
    compositor_cancel: CancellationToken,
    compositor_task: Option<JoinHandle<()>>,
    pub keyboard: KeyboardHandle,
    keyboard_sender: ServiceSender<Keyboard>,
    keyboard_cancel: CancellationToken,
    keyboard_task: Option<JoinHandle<()>>,
    pub calendar: CalendarHandle,
    calendar_sender: ServiceSender<Calendar>,
    calendar_cancel: CancellationToken,
    calendar_task: Option<JoinHandle<()>>,
    pub mpris: MprisHandle,
    mpris_sender: ServiceSender<Mpris>,
    mpris_cancel: CancellationToken,
    mpris_task: Option<JoinHandle<()>>,
    pub heartbeat: HeartbeatHandle,
    heartbeat_sender: ServiceSender<Heartbeat>,
    heartbeat_cancel: CancellationToken,
    heartbeat_task: Option<JoinHandle<()>>,
    pub notifications: NotificationsProviderHandle,
    notifications_task: Option<JoinHandle<()>>,
}

impl PanelServices {
    pub async fn start(document: &Config) -> Self {
        Self::start_with_buses(document, Buses::connect().await)
    }

    fn start_with_buses(document: &Config, buses: Buses) -> Self {
        let (notifications, notifications_task) = match buses.session_bus() {
            Ok(connection) => {
                let (handle, task) = NotificationsProviderHandle::start(connection.clone());
                (handle, Some(task))
            }
            Err(reason) => (NotificationsProviderHandle::unavailable(reason), None),
        };
        let compositor_cancel = CancellationToken::new();
        let (compositor_runtime, compositor) = ServiceRuntime::<Compositor>::new(
            initial_compositor_state(),
            buses.clone(),
            compositor_cancel.clone(),
        );
        let compositor_sender = compositor_runtime.sender();
        let compositor_task = spawn_service(document, compositor_runtime, ());

        let keyboard_cancel = CancellationToken::new();
        let (keyboard_runtime, keyboard) = ServiceRuntime::<Keyboard>::new(
            KeyboardLayouts {
                layouts: Vec::new(),
                current: None,
            },
            buses.clone(),
            keyboard_cancel.clone(),
        );
        let keyboard_sender = keyboard_runtime.sender();
        let keyboard_task = spawn_service(
            document,
            keyboard_runtime,
            KeyboardDependencies {
                compositor: compositor.clone(),
            },
        );
        let calendar_cancel = CancellationToken::new();
        let (calendar_runtime, calendar) = ServiceRuntime::<Calendar>::new(
            initial_calendar_state(),
            buses.clone(),
            calendar_cancel.clone(),
        );
        let calendar_sender = calendar_runtime.sender();
        let calendar_task = spawn_service(document, calendar_runtime, ());

        let mpris_cancel = CancellationToken::new();
        let (mpris_runtime, mpris) = ServiceRuntime::<Mpris>::new(
            initial_mpris_state(),
            buses.clone(),
            mpris_cancel.clone(),
        );
        let mpris_sender = mpris_runtime.sender();
        let mpris_task = spawn_service(document, mpris_runtime, ());

        let heartbeat_cancel = CancellationToken::new();
        let (heartbeat_runtime, heartbeat) = ServiceRuntime::<Heartbeat>::new(
            Heartbeat::initial_state(),
            buses,
            heartbeat_cancel.clone(),
        );
        let heartbeat_sender = heartbeat_runtime.sender();
        let heartbeat_task = spawn_service(document, heartbeat_runtime, ());

        Self {
            compositor,
            compositor_sender,
            compositor_cancel,
            compositor_task: Some(compositor_task),
            keyboard,
            keyboard_sender,
            keyboard_cancel,
            keyboard_task: Some(keyboard_task),
            calendar,
            calendar_sender,
            calendar_cancel,
            calendar_task: Some(calendar_task),
            mpris,
            mpris_sender,
            mpris_cancel,
            mpris_task: Some(mpris_task),
            heartbeat,
            heartbeat_sender,
            heartbeat_cancel,
            heartbeat_task: Some(heartbeat_task),
            notifications,
            notifications_task,
        }
    }

    pub async fn shutdown(mut self) {
        if let Some(task) = self.notifications_task.take() {
            task.abort();
            let _ = task.await;
        }
        stop(
            Heartbeat::NAME,
            &self.heartbeat_cancel,
            &mut self.heartbeat_task,
        )
        .await;
        stop(Mpris::NAME, &self.mpris_cancel, &mut self.mpris_task).await;
        stop(
            Calendar::NAME,
            &self.calendar_cancel,
            &mut self.calendar_task,
        )
        .await;
        stop(
            Keyboard::NAME,
            &self.keyboard_cancel,
            &mut self.keyboard_task,
        )
        .await;
        stop(
            Compositor::NAME,
            &self.compositor_cancel,
            &mut self.compositor_task,
        )
        .await;
    }

    pub fn reconfigure(&self, document: &Config) {
        self.compositor_sender
            .reconfigure(<Compositor as Service>::Config::from(document));
        self.keyboard_sender
            .reconfigure(<Keyboard as Service>::Config::from(document));
        self.calendar_sender
            .reconfigure(<Calendar as Service>::Config::from(document));
        self.mpris_sender
            .reconfigure(<Mpris as Service>::Config::from(document));
        self.heartbeat_sender
            .reconfigure(<Heartbeat as Service>::Config::from(document));
    }

    fn cancel(&self) {
        if let Some(task) = &self.notifications_task {
            task.abort();
        }
        self.heartbeat_cancel.cancel();
        self.mpris_cancel.cancel();
        self.calendar_cancel.cancel();
        self.keyboard_cancel.cancel();
        self.compositor_cancel.cancel();
    }
}

impl Drop for PanelServices {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl fmt::Debug for PanelServices {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PanelServices")
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
    async fn the_panel_owns_one_typed_local_service_graph() {
        let services = PanelServices::start_with_buses(
            &Config::default(),
            Buses::unavailable("no bus in tests"),
        );
        let health = [
            services.compositor.health(),
            services.keyboard.health(),
            services.calendar.health(),
            services.mpris.health(),
            services.heartbeat.health(),
        ];

        services.shutdown().await;

        for state in health {
            assert!(matches!(&*state.borrow(), ServiceState::Stopped { .. }));
        }
    }
}
