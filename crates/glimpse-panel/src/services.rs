use std::fmt;

use glimpse_config::Config;
use glimpse_dbus::{
    Buses,
    notifications::{NotificationsProvider, NotificationsProviderHandle},
    weather::{WeatherProvider, WeatherProviderHandle},
};
use glimpse_services::{
    Calendar, CalendarHandle, Compositor, CompositorHandle, Heartbeat, HeartbeatHandle, Keyboard,
    KeyboardDependencies, KeyboardHandle, Mpris, MprisHandle, Running, Tray, TrayHandle,
};

pub struct PanelServices {
    pub compositor: CompositorHandle,
    pub keyboard: KeyboardHandle,
    pub calendar: CalendarHandle,
    pub mpris: MprisHandle,
    pub heartbeat: HeartbeatHandle,
    pub tray: TrayHandle,
    compositor_service: Running<Compositor>,
    keyboard_service: Running<Keyboard>,
    calendar_service: Running<Calendar>,
    mpris_service: Running<Mpris>,
    heartbeat_service: Running<Heartbeat>,
    tray_service: Running<Tray>,
    notifications: NotificationsProvider,
    weather: WeatherProvider,
}

impl PanelServices {
    pub async fn start(document: &Config) -> Self {
        Self::start_with_buses(document, Buses::connect().await)
    }

    fn start_with_buses(document: &Config, buses: Buses) -> Self {
        let (notifications, weather) = match buses.session_bus() {
            Ok(connection) => (
                NotificationsProvider::start(connection.clone()),
                WeatherProvider::start(connection.clone()),
            ),
            Err(reason) => (
                NotificationsProvider::unavailable(reason),
                WeatherProvider::unavailable(reason),
            ),
        };
        let (compositor_service, compositor) =
            Running::<Compositor>::spawn(document, buses.clone(), ());
        let (keyboard_service, keyboard) = Running::spawn(
            document,
            buses.clone(),
            KeyboardDependencies {
                compositor: compositor.clone(),
            },
        );
        let (calendar_service, calendar) = Running::<Calendar>::spawn(document, buses.clone(), ());
        let (mpris_service, mpris) = Running::<Mpris>::spawn(document, buses.clone(), ());
        let (heartbeat_service, heartbeat) =
            Running::<Heartbeat>::spawn(document, buses.clone(), ());
        let (tray_service, tray) = Running::<Tray>::spawn(document, buses, ());

        Self {
            compositor,
            keyboard,
            calendar,
            mpris,
            heartbeat,
            tray,
            compositor_service,
            keyboard_service,
            calendar_service,
            mpris_service,
            heartbeat_service,
            tray_service,
            notifications,
            weather,
        }
    }

    pub async fn shutdown(mut self) {
        self.cancel();
        self.weather.shutdown().await;
        self.notifications.shutdown().await;
        self.tray_service.stop().await;
        self.heartbeat_service.stop().await;
        self.mpris_service.stop().await;
        self.calendar_service.stop().await;
        self.keyboard_service.stop().await;
        self.compositor_service.stop().await;
    }

    pub fn reconfigure(&self, document: &Config) {
        self.compositor_service.reconfigure(document);
        self.keyboard_service.reconfigure(document);
        self.calendar_service.reconfigure(document);
        self.mpris_service.reconfigure(document);
        self.heartbeat_service.reconfigure(document);
        self.tray_service.reconfigure(document);
    }

    pub fn notifications(&self) -> NotificationsProviderHandle {
        self.notifications.handle()
    }

    pub fn weather(&self) -> WeatherProviderHandle {
        self.weather.handle()
    }

    fn cancel(&self) {
        self.tray_service.cancel();
        self.heartbeat_service.cancel();
        self.mpris_service.cancel();
        self.calendar_service.cancel();
        self.keyboard_service.cancel();
        self.compositor_service.cancel();
    }
}

impl fmt::Debug for PanelServices {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PanelServices")
            .finish_non_exhaustive()
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
            services.tray.health(),
        ];

        services.shutdown().await;

        for state in health {
            assert!(matches!(&*state.borrow(), ServiceState::Stopped { .. }));
        }
    }
}
