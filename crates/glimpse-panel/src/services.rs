use std::fmt;
use std::sync::Arc;

use glimpse_config::Config;
use glimpse_dbus::{
    Buses,
    night_light::{NightLightProvider, NightLightProviderHandle},
    notifications::{NotificationsProvider, NotificationsProviderHandle},
    weather::{WeatherProvider, WeatherProviderHandle},
};
use glimpse_services::{
    Audio, AudioHandle, Backlight, Bluetooth, BluetoothHandle, Brightness, BrightnessDependencies,
    BrightnessHandle, Calendar, CalendarHandle, Compositor, CompositorHandle, Heartbeat,
    HeartbeatHandle, Keyboard, KeyboardDependencies, KeyboardHandle, Mpris, MprisHandle, Network,
    NetworkHandle, Running, SysfsBacklight, Tray, TrayHandle, UnavailableBacklight,
};

pub struct PanelServices {
    pub compositor: CompositorHandle,
    pub keyboard: KeyboardHandle,
    pub calendar: CalendarHandle,
    pub mpris: MprisHandle,
    pub heartbeat: HeartbeatHandle,
    pub tray: TrayHandle,
    pub bluetooth: BluetoothHandle,
    pub network: NetworkHandle,
    pub audio: AudioHandle,
    pub brightness: BrightnessHandle,
    compositor_service: Running<Compositor>,
    keyboard_service: Running<Keyboard>,
    calendar_service: Running<Calendar>,
    mpris_service: Running<Mpris>,
    heartbeat_service: Running<Heartbeat>,
    tray_service: Running<Tray>,
    bluetooth_service: Running<Bluetooth>,
    network_service: Running<Network>,
    audio_service: Running<Audio>,
    brightness_service: Running<Brightness>,
    notifications: NotificationsProvider,
    weather: WeatherProvider,
    night_light: NightLightProvider,
}

impl PanelServices {
    pub async fn start(document: &Config) -> Self {
        Self::start_with_buses(document, Buses::connect().await)
    }

    pub(crate) fn start_with_buses(document: &Config, buses: Buses) -> Self {
        let (notifications, weather, night_light) = match buses.session_bus() {
            Ok(connection) => (
                NotificationsProvider::start(connection.clone()),
                WeatherProvider::start(connection.clone()),
                NightLightProvider::start(connection.clone()),
            ),
            Err(reason) => (
                NotificationsProvider::unavailable(reason),
                WeatherProvider::unavailable(reason),
                NightLightProvider::unavailable(reason),
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
        let (tray_service, tray) = Running::<Tray>::spawn(document, buses.clone(), ());
        let (bluetooth_service, bluetooth) =
            Running::<Bluetooth>::spawn(document, buses.clone(), ());
        let (network_service, network) = Running::<Network>::spawn(document, buses.clone(), ());
        let (audio_service, audio) = Running::<Audio>::spawn(document, buses.clone(), ());
        let backend: Arc<dyn Backlight> = match buses.system_bus() {
            Ok(bus) => Arc::new(SysfsBacklight::new(bus.clone())),
            Err(_) => Arc::new(UnavailableBacklight),
        };
        let (brightness_service, brightness) =
            Running::<Brightness>::spawn(document, buses, BrightnessDependencies { backend });

        Self {
            compositor,
            keyboard,
            calendar,
            mpris,
            heartbeat,
            tray,
            bluetooth,
            network,
            audio,
            brightness,
            compositor_service,
            keyboard_service,
            calendar_service,
            mpris_service,
            heartbeat_service,
            tray_service,
            bluetooth_service,
            network_service,
            audio_service,
            brightness_service,
            notifications,
            weather,
            night_light,
        }
    }

    pub async fn shutdown(mut self) {
        self.cancel();
        self.weather.shutdown().await;
        self.notifications.shutdown().await;
        self.night_light.shutdown().await;
        self.brightness_service.stop().await;
        self.audio_service.stop().await;
        self.bluetooth_service.stop().await;
        self.network_service.stop().await;
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
        self.bluetooth_service.reconfigure(document);
        self.network_service.reconfigure(document);
        self.audio_service.reconfigure(document);
        self.brightness_service.reconfigure(document);
    }

    pub fn notifications(&self) -> NotificationsProviderHandle {
        self.notifications.handle()
    }

    pub fn weather(&self) -> WeatherProviderHandle {
        self.weather.handle()
    }

    pub fn night_light(&self) -> NightLightProviderHandle {
        self.night_light.handle()
    }

    fn cancel(&self) {
        self.brightness_service.cancel();
        self.audio_service.cancel();
        self.bluetooth_service.cancel();
        self.network_service.cancel();
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
