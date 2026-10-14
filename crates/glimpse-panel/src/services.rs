use std::fmt;
use std::sync::Arc;

use glimpse_config::Config;
use glimpse_dbus::{
    Buses,
    idle::{IdleProvider, IdleProviderHandle},
    night_light::{NightLightProvider, NightLightProviderHandle},
    notifications::{NotificationsProvider, NotificationsProviderHandle},
    weather::{WeatherProvider, WeatherProviderHandle},
};
use glimpse_services::SystemdLauncher;
use glimpse_services::{
    Audio, AudioHandle, Backlight, Battery, BatteryDependencies, BatteryHandle, BatterySource,
    Bluetooth, BluetoothDependencies, BluetoothHandle, Brightness, BrightnessDependencies,
    BrightnessHandle, Calendar, CalendarHandle, Clipboard, ClipboardDependencies, ClipboardHandle,
    ColorPicker, ColorPickerDependencies, ColorPickerHandle, CompositeBacklight, Compositor,
    CompositorHandle, DdcBacklight, DesktopCatalog, Exec, ExecDependencies, ExecHandle, Heartbeat,
    HeartbeatHandle, Kdeconnect, KdeconnectHandle, Keyboard, KeyboardDependencies, KeyboardHandle,
    Mpris, MprisHandle, Network, NetworkDependencies, NetworkHandle, Places, PlacesHandle,
    PortalPicker, Printing, PrintingHandle, Privacy, PrivacyDependencies, PrivacyHandle,
    ProcessRulerRunner, Removable, RemovableHandle, Ruler, RulerDependencies, RulerHandle, Running,
    Selection, SessionActions, SessionActionsDependencies, SessionActionsHandle, SysfsBacklight,
    SystemMonitor, SystemMonitorHandle, Tray, TrayHandle, UnavailableBacklight,
    kdeconnect_batteries,
};

pub struct PanelServices {
    pub session: Option<zbus::Connection>,
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
    pub session_actions: SessionActionsHandle,
    pub battery: BatteryHandle,
    pub clipboard: ClipboardHandle,
    pub color_picker: ColorPickerHandle,
    pub places: PlacesHandle,
    pub printing: PrintingHandle,
    pub removable: RemovableHandle,
    pub kdeconnect: KdeconnectHandle,
    pub privacy: PrivacyHandle,
    pub system_monitor: SystemMonitorHandle,
    pub ruler: RulerHandle,
    exec: ExecHandle,
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
    session_actions_service: Running<SessionActions>,
    battery_service: Running<Battery>,
    clipboard_service: Running<Clipboard>,
    color_picker_service: Running<ColorPicker>,
    places_service: Running<Places>,
    printing_service: Running<Printing>,
    removable_service: Running<Removable>,
    kdeconnect_service: Running<Kdeconnect>,
    privacy_service: Running<Privacy>,
    system_monitor_service: Running<SystemMonitor>,
    ruler_service: Running<Ruler>,
    exec_service: Running<Exec>,
    notifications: NotificationsProvider,
    weather: WeatherProvider,
    night_light: NightLightProvider,
    idle: IdleProvider,
    clipboard_served: Option<crate::clipboard_provider::Served>,
}

impl PanelServices {
    pub async fn start(document: &Config) -> Self {
        Self::start_with_buses(document, Buses::connect().await)
    }

    pub(crate) fn start_with_buses(document: &Config, buses: Buses) -> Self {
        let session = buses.session_bus().ok().cloned();
        let (notifications, weather, night_light, idle) = match buses.session_bus() {
            Ok(connection) => (
                NotificationsProvider::start(connection.clone()),
                WeatherProvider::start(connection.clone()),
                NightLightProvider::start(connection.clone()),
                IdleProvider::start(connection.clone()),
            ),
            Err(reason) => (
                NotificationsProvider::unavailable(reason),
                WeatherProvider::unavailable(reason),
                NightLightProvider::unavailable(reason),
                IdleProvider::unavailable(reason),
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
        let (bluetooth_service, bluetooth) = Running::<Bluetooth>::spawn(
            document,
            buses.clone(),
            BluetoothDependencies { agent: true },
        );
        let (network_service, network) =
            Running::<Network>::spawn(document, buses.clone(), NetworkDependencies { agent: true });
        let (audio_service, audio) = Running::<Audio>::spawn(document, buses.clone(), ());
        let sysfs: Arc<dyn Backlight> = match buses.system_bus() {
            Ok(bus) => Arc::new(SysfsBacklight::new(bus.clone())),
            Err(_) => Arc::new(UnavailableBacklight),
        };
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
        let (color_picker_service, color_picker) = Running::<ColorPicker>::spawn(
            document,
            buses.clone(),
            ColorPickerDependencies {
                selection: Arc::clone(&selection),
                picker: Arc::new(PortalPicker::new(buses.clone())),
            },
        );
        let (ruler_service, ruler) = Running::<Ruler>::spawn(
            document,
            buses.clone(),
            RulerDependencies {
                selection: Arc::clone(&selection),
                runner: Arc::new(ProcessRulerRunner::new("glimpse-lens")),
            },
        );
        let (exec_service, exec) = Running::<Exec>::spawn(
            document,
            buses.clone(),
            ExecDependencies {
                catalog: Arc::new(DesktopCatalog),
                selection,
                notifications: notifications.handle(),
                mpris: mpris.clone(),
                compositor: compositor.clone(),
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
        let (system_monitor_service, system_monitor) =
            Running::<SystemMonitor>::spawn(document, buses.clone(), ());
        let (removable_service, removable) =
            Running::<Removable>::spawn(document, buses.clone(), ());

        Self {
            session,
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
            session_actions,
            battery,
            clipboard,
            color_picker,
            places,
            printing,
            removable,
            kdeconnect,
            privacy,
            system_monitor,
            ruler,
            exec,
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
            session_actions_service,
            battery_service,
            clipboard_service,
            color_picker_service,
            places_service,
            printing_service,
            removable_service,
            kdeconnect_service,
            privacy_service,
            system_monitor_service,
            ruler_service,
            exec_service,
            notifications,
            weather,
            night_light,
            idle,
            clipboard_served,
        }
    }

    pub async fn shutdown(mut self) {
        self.cancel();
        self.weather.shutdown().await;
        self.idle.shutdown().await;
        self.notifications.shutdown().await;
        self.night_light.shutdown().await;
        self.system_monitor_service.stop().await;
        self.removable_service.stop().await;
        self.kdeconnect_service.stop().await;
        self.printing_service.stop().await;
        self.places_service.stop().await;
        self.ruler_service.stop().await;
        self.exec_service.stop().await;
        self.color_picker_service.stop().await;
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
        self.clipboard_service.reconfigure(document);
        self.ruler_service.reconfigure(document);
        self.exec_service.reconfigure(document);
        self.session_actions_service.reconfigure(document);
        self.battery_service.reconfigure(document);
        self.places_service.reconfigure(document);
        self.printing_service.reconfigure(document);
        self.removable_service.reconfigure(document);
        self.kdeconnect_service.reconfigure(document);
        self.privacy_service.reconfigure(document);
        self.system_monitor_service.reconfigure(document);
    }

    pub fn notifications(&self) -> NotificationsProviderHandle {
        self.notifications.handle()
    }

    pub fn exec(&self) -> ExecHandle {
        self.exec.clone()
    }

    pub fn weather(&self) -> WeatherProviderHandle {
        self.weather.handle()
    }

    pub fn night_light(&self) -> NightLightProviderHandle {
        self.night_light.handle()
    }

    pub fn idle(&self) -> IdleProviderHandle {
        self.idle.handle()
    }

    fn cancel(&self) {
        self.system_monitor_service.cancel();
        self.removable_service.cancel();
        self.kdeconnect_service.cancel();
        self.printing_service.cancel();
        self.places_service.cancel();
        self.ruler_service.cancel();
        self.exec_service.cancel();
        self.color_picker_service.cancel();
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
        self.calendar_service.cancel();
        self.keyboard_service.cancel();
        self.compositor_service.cancel();
    }
}

impl fmt::Debug for PanelServices {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PanelServices")
            .field("exec_slots", &self.exec().snapshot().slots.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
