use std::fmt;
use std::sync::Arc;

use glimpse_config::Config;
use glimpse_dbus::{
    Buses,
    calendar::{CalendarProvider, CalendarProviderHandle},
    idle::{IdleProvider, IdleProviderHandle},
    night_light::{NightLightProvider, NightLightProviderHandle},
    notifications::{NotificationsProvider, NotificationsProviderHandle},
    weather::{WeatherProvider, WeatherProviderHandle},
};
use glimpse_services::SystemdLauncher;
use glimpse_services::{
    Audio, AudioHandle, Backlight, Battery, BatteryDependencies, BatteryHandle, BatterySource,
    Bluetooth, BluetoothHandle, Brightness, BrightnessDependencies, BrightnessHandle, Clipboard,
    ClipboardDependencies, ClipboardHandle, CompositeBacklight, Compositor, CompositorDependencies,
    CompositorHandle, CompositorProvider, CompositorProviderHandle, DdcBacklight, DesktopCatalog,
    Exec, ExecDependencies, ExecHandle, Heartbeat, HeartbeatHandle, Kdeconnect, KdeconnectHandle,
    Keyboard, KeyboardDependencies, KeyboardHandle, Mpris, MprisHandle, Network, NetworkHandle,
    Places, PlacesHandle, Printing, PrintingHandle, Privacy, PrivacyDependencies, PrivacyHandle,
    Removable, RemovableHandle, Running, Selection, SessionActions, SessionActionsDependencies,
    SessionActionsHandle, SysfsBacklight, Tray, TrayHandle, kdeconnect_batteries,
};

#[derive(Clone)]
pub struct Handles {
    pub compositor: CompositorHandle,
    pub keyboard: KeyboardHandle,
    pub calendar: CalendarProviderHandle,
    pub mpris: MprisHandle,
    pub heartbeat: HeartbeatHandle,
    pub tray: TrayHandle,
    pub bluetooth: BluetoothHandle,
    pub network: NetworkHandle,
    pub audio: AudioHandle,
    pub brightness: BrightnessHandle,
    pub night_light: NightLightProviderHandle,
    pub notifications: NotificationsProviderHandle,
    pub weather: WeatherProviderHandle,
    pub idle: IdleProviderHandle,
    pub session_actions: SessionActionsHandle,
    pub battery: BatteryHandle,
    pub clipboard: ClipboardHandle,
    pub places: PlacesHandle,
    pub printing: PrintingHandle,
    pub removable: RemovableHandle,
    pub kdeconnect: KdeconnectHandle,
    pub privacy: PrivacyHandle,
    pub exec: ExecHandle,
}

pub struct PanelServices {
    pub session: Option<zbus::Connection>,
    pub handles: Handles,
    compositor_service: Running<Compositor>,
    keyboard_service: Running<Keyboard>,
    mpris_service: Running<Mpris>,
    heartbeat_service: Running<Heartbeat>,
    tray_service: Running<Tray>,
    bluetooth_service: Running<Bluetooth>,
    network_service: Running<Network>,
    audio_service: Running<Audio>,
    brightness_service: Running<Brightness>,
    session_actions_service: Running<SessionActions>,
    battery_service: Running<Battery>,
    clipboard_service: Running<Clipboard>,
    places_service: Running<Places>,
    printing_service: Running<Printing>,
    removable_service: Running<Removable>,
    kdeconnect_service: Running<Kdeconnect>,
    privacy_service: Running<Privacy>,
    exec_service: Running<Exec>,
    notifications: NotificationsProvider,
    weather: WeatherProvider,
    calendar: CalendarProvider,
    night_light: NightLightProvider,
    idle: IdleProvider,
    compositor_provider: CompositorProvider,
    clipboard_served: Option<crate::clipboard_provider::Served>,
}

impl PanelServices {
    pub async fn start(document: &Config) -> Self {
        Self::start_with_buses(document, Buses::connect().await)
    }

    pub(crate) fn start_with_buses(document: &Config, buses: Buses) -> Self {
        let session = buses.session_bus().ok().cloned();
        let (notifications, weather, calendar, night_light, idle, compositor_provider) =
            match buses.session_bus() {
                Ok(connection) => (
                    NotificationsProvider::start(connection.clone()),
                    WeatherProvider::start(connection.clone()),
                    CalendarProvider::start(connection.clone()),
                    NightLightProvider::start(connection.clone()),
                    IdleProvider::start(connection.clone()),
                    CompositorProvider::start(connection.clone()),
                ),
                Err(reason) => (
                    NotificationsProvider::unavailable(reason),
                    WeatherProvider::unavailable(reason),
                    CalendarProvider::unavailable(reason),
                    NightLightProvider::unavailable(reason),
                    IdleProvider::unavailable(reason),
                    CompositorProvider::unavailable(reason),
                ),
            };
        let (compositor_service, compositor) = Running::<Compositor>::spawn(
            document,
            buses.clone(),
            CompositorDependencies {
                provider: CompositorProviderHandle(compositor_provider.handle()),
            },
        );
        let (keyboard_service, keyboard) = Running::spawn(
            document,
            buses.clone(),
            KeyboardDependencies {
                provider: CompositorProviderHandle(compositor_provider.handle()),
            },
        );
        let (mpris_service, mpris) = Running::<Mpris>::spawn(document, buses.clone(), ());
        let (heartbeat_service, heartbeat) =
            Running::<Heartbeat>::spawn(document, buses.clone(), ());
        let (tray_service, tray) = Running::<Tray>::spawn(document, buses.clone(), ());
        let (bluetooth_service, bluetooth) =
            Running::<Bluetooth>::spawn(document, buses.clone(), ());
        let (network_service, network) = Running::<Network>::spawn(document, buses.clone(), ());
        let (audio_service, audio) = Running::<Audio>::spawn(document, buses.clone(), ());
        let sysfs: Arc<dyn Backlight> = Arc::new(SysfsBacklight::new(buses.clone()));
        let backend: Arc<dyn Backlight> = if document.brightness.ddc {
            Arc::new(CompositeBacklight::new(
                sysfs,
                Arc::new(DdcBacklight::new()),
            ))
        } else {
            sysfs
        };
        let (brightness_service, brightness) = Running::<Brightness>::spawn(
            document,
            buses.clone(),
            BrightnessDependencies { backend },
        );
        let (session_actions_service, session_actions) = Running::<SessionActions>::spawn(
            document,
            buses.clone(),
            SessionActionsDependencies {
                compositor: compositor.clone(),
            },
        );
        let selection: Arc<dyn Selection> = Arc::new(crate::selection::WaylandSelection::new());
        let (clipboard_service, clipboard) = Running::<Clipboard>::spawn(
            document,
            buses.clone(),
            ClipboardDependencies {
                selection: Arc::clone(&selection),
            },
        );
        let clipboard_served = buses.session_bus().ok().map(|connection| {
            crate::clipboard_provider::Served::start(connection.clone(), clipboard.clone())
        });
        let (exec_service, exec) = Running::<Exec>::spawn(
            document,
            buses.clone(),
            ExecDependencies {
                catalog: Arc::new(DesktopCatalog),
                launcher: Arc::new(SystemdLauncher::new(session.clone())),
            },
        );
        let (kdeconnect_service, kdeconnect) =
            Running::<Kdeconnect>::spawn(document, buses.clone(), ());
        let (battery_service, battery) = Running::<Battery>::spawn(
            document,
            buses.clone(),
            BatteryDependencies {
                sources: vec![BatterySource::new(
                    kdeconnect.subscribe(),
                    kdeconnect_batteries,
                )],
            },
        );
        let (places_service, places) = Running::<Places>::spawn(document, buses.clone(), ());
        let (printing_service, printing) = Running::<Printing>::spawn(document, buses.clone(), ());
        let (privacy_service, privacy) = Running::<Privacy>::spawn(
            document,
            buses.clone(),
            PrivacyDependencies {
                audio: audio.clone(),
                compositor: compositor.clone(),
            },
        );
        let (removable_service, removable) =
            Running::<Removable>::spawn(document, buses.clone(), ());

        Self {
            session,
            handles: Handles {
                compositor,
                keyboard,
                mpris,
                heartbeat,
                tray,
                bluetooth,
                network,
                audio,
                brightness,
                session_actions,
                battery,
                clipboard,
                places,
                printing,
                removable,
                kdeconnect,
                privacy,
                exec,
                night_light: night_light.handle(),
                notifications: notifications.handle(),
                weather: weather.handle(),
                calendar: calendar.handle(),
                idle: idle.handle(),
            },
            compositor_service,
            keyboard_service,
            mpris_service,
            heartbeat_service,
            tray_service,
            bluetooth_service,
            network_service,
            audio_service,
            brightness_service,
            session_actions_service,
            battery_service,
            clipboard_service,
            places_service,
            printing_service,
            removable_service,
            kdeconnect_service,
            privacy_service,
            exec_service,
            notifications,
            weather,
            calendar,
            night_light,
            idle,
            compositor_provider,
            clipboard_served,
        }
    }

    pub async fn shutdown(mut self) {
        self.cancel();
        self.weather.shutdown().await;
        self.calendar.shutdown().await;
        self.idle.shutdown().await;
        self.notifications.shutdown().await;
        self.night_light.shutdown().await;
        self.removable_service.stop().await;
        self.kdeconnect_service.stop().await;
        self.printing_service.stop().await;
        self.places_service.stop().await;
        self.exec_service.stop().await;
        if let Some(served) = self.clipboard_served.take() {
            served.shutdown().await;
        }
        self.clipboard_service.stop().await;
        self.brightness_service.stop().await;
        self.session_actions_service.stop().await;
        self.battery_service.stop().await;
        self.privacy_service.stop().await;
        self.audio_service.stop().await;
        self.bluetooth_service.stop().await;
        self.network_service.stop().await;
        self.tray_service.stop().await;
        self.heartbeat_service.stop().await;
        self.mpris_service.stop().await;
        self.keyboard_service.stop().await;
        self.compositor_service.stop().await;
        self.compositor_provider.shutdown().await;
    }

    pub fn reconfigure(&self, document: &Config) {
        self.compositor_service.reconfigure(document);
        self.keyboard_service.reconfigure(document);
        self.mpris_service.reconfigure(document);
        self.heartbeat_service.reconfigure(document);
        self.tray_service.reconfigure(document);
        self.bluetooth_service.reconfigure(document);
        self.network_service.reconfigure(document);
        self.audio_service.reconfigure(document);
        self.brightness_service.reconfigure(document);
        self.clipboard_service.reconfigure(document);
        self.exec_service.reconfigure(document);
        self.session_actions_service.reconfigure(document);
        self.battery_service.reconfigure(document);
        self.places_service.reconfigure(document);
        self.printing_service.reconfigure(document);
        self.removable_service.reconfigure(document);
        self.kdeconnect_service.reconfigure(document);
        self.privacy_service.reconfigure(document);
    }

    fn cancel(&self) {
        self.removable_service.cancel();
        self.kdeconnect_service.cancel();
        self.printing_service.cancel();
        self.places_service.cancel();
        self.exec_service.cancel();
        self.clipboard_service.cancel();
        self.brightness_service.cancel();
        self.session_actions_service.cancel();
        self.battery_service.cancel();
        self.privacy_service.cancel();
        self.audio_service.cancel();
        self.bluetooth_service.cancel();
        self.network_service.cancel();
        self.tray_service.cancel();
        self.heartbeat_service.cancel();
        self.mpris_service.cancel();
        self.keyboard_service.cancel();
        self.compositor_service.cancel();
    }
}

impl fmt::Debug for PanelServices {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PanelServices")
            .field("exec_slots", &self.handles.exec.snapshot().slots.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
