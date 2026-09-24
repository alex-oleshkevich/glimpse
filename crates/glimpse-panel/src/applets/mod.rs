mod agenda;
pub mod audio;
mod battery;
pub(crate) mod bluetooth;
mod brightness;
mod clipboard;
mod clock;
mod color_picker;
mod command;
mod display;
mod heartbeat;
pub(crate) mod idle;
mod kdeconnect;
mod keyboard;
mod mpris;
pub mod network;
mod next_event;
mod notifications;
mod pager;
mod places;
mod printing;
mod privacy;
mod removable;
mod ruler;
mod session;
mod system_monitor;
mod tokens;
mod tray;
pub(crate) mod weather;
mod workspace;
mod workspace_name;

use glimpse_config::{Applet as AppletConfig, AppletKind, Regional};
use glimpse_dbus::{
    idle::IdleProviderHandle, night_light::NightLightProviderHandle,
    notifications::NotificationsProviderHandle, weather::WeatherProviderHandle,
};
use glimpse_services::{
    AudioHandle, BatteryHandle, BluetoothHandle, BrightnessHandle, CalendarHandle, ClipboardHandle,
    ColorPickerHandle, CompositorHandle, HeartbeatHandle, KdeconnectHandle, KeyboardHandle,
    MprisHandle, NetworkHandle, PlacesHandle, PrintingHandle, PrivacyHandle, RemovableHandle,
    RulerHandle, SessionActionsHandle, SystemMonitorHandle, TrayHandle,
};
use std::collections::BTreeMap;

use crate::applet::runtime::Builder;

pub fn configured(
    name: &str,
    configured: &BTreeMap<String, AppletConfig>,
    regional: &Regional,
) -> Option<AppletConfig> {
    let Some(mut config) = glimpse_config::resolve_applet(name, configured) else {
        tracing::warn!(applet = name, "unknown applet, skipping");
        return None;
    };
    config.regional = regional.clone();
    Some(config)
}

#[allow(clippy::too_many_arguments)]
pub fn build(
    config: &AppletConfig,
    compositor: &CompositorHandle,
    keyboard: &KeyboardHandle,
    calendar: &CalendarHandle,
    mpris: &MprisHandle,
    heartbeat: &HeartbeatHandle,
    tray: &TrayHandle,
    bluetooth: &BluetoothHandle,
    network: &NetworkHandle,
    audio: &AudioHandle,
    brightness: &BrightnessHandle,
    night_light: &NightLightProviderHandle,
    notifications: &NotificationsProviderHandle,
    weather: &WeatherProviderHandle,
    idle: &IdleProviderHandle,
    color_picker: &ColorPickerHandle,
    session_actions: &SessionActionsHandle,
    battery: &BatteryHandle,
    clipboard: &ClipboardHandle,
    places: &PlacesHandle,
    printing: &PrintingHandle,
    removable: &RemovableHandle,
    kdeconnect: &KdeconnectHandle,
    privacy: &PrivacyHandle,
    system_monitor: &SystemMonitorHandle,
    ruler: &RulerHandle,
    dialog: Option<&relm4::Sender<crate::app::AppInput>>,
) -> Option<Builder> {
    match &config.kind {
        AppletKind::Clock(_) => {
            let calendar = calendar.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(calendar.subscribe());
                Box::new(clock::Clock::start(calendar))
            }))
        }
        AppletKind::Tray(_) => {
            let tray = tray.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(tray.subscribe());
                Box::new(tray::Tray::start(tray))
            }))
        }
        AppletKind::Heartbeat {} => {
            let heartbeat = heartbeat.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(heartbeat.subscribe());
                Box::new(heartbeat::Heartbeat::start(heartbeat))
            }))
        }
        AppletKind::Mpris(_) => {
            let mpris = mpris.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(mpris.subscribe());
                Box::new(mpris::Mpris::start(mpris))
            }))
        }
        AppletKind::NextEvent(_) => {
            let calendar = calendar.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(calendar.subscribe());
                Box::new(next_event::NextEvent::start(calendar))
            }))
        }
        AppletKind::Pager(_) => {
            let compositor = compositor.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(compositor.subscribe());
                Box::new(pager::Pager::start(compositor))
            }))
        }
        AppletKind::Weather(_) => {
            let weather = weather.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(weather.subscribe());
                Box::new(weather::Weather::start(weather))
            }))
        }
        AppletKind::Keyboard {} => {
            let keyboard = keyboard.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(keyboard.subscribe());
                Box::new(keyboard::Keyboard::start(keyboard))
            }))
        }
        AppletKind::Notifications(_) => {
            let notifications = notifications.clone();
            let compositor = compositor.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(notifications.subscribe());
                Box::new(notifications::Notifications::start(
                    notifications,
                    compositor,
                ))
            }))
        }
        AppletKind::Bluetooth(_) => {
            let bluetooth = bluetooth.clone();
            let notifications = notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(bluetooth.subscribe());
                Box::new(bluetooth::Bluetooth::start(bluetooth, notifications))
            }))
        }
        AppletKind::Network(_) => {
            let network = network.clone();
            let notifications = notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(network.subscribe());
                Box::new(network::Network::start(network, notifications))
            }))
        }
        AppletKind::Audio {} => {
            let audio = audio.clone();
            let notifications = notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(audio.subscribe());
                Box::new(audio::Audio::start(audio, notifications))
            }))
        }
        AppletKind::Brightness(_) => {
            let brightness = brightness.clone();
            let night_light = night_light.clone();
            let compositor = compositor.clone();
            let notifications = notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(brightness.subscribe());
                ctx.watch(night_light.subscribe());
                ctx.watch(compositor.subscribe());
                Box::new(brightness::Brightness::start(
                    brightness,
                    night_light,
                    compositor,
                    notifications,
                ))
            }))
        }
        AppletKind::Display {} => {
            let compositor = compositor.clone();
            let notifications = notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(compositor.subscribe());
                Box::new(display::Display::start(compositor, notifications))
            }))
        }
        AppletKind::Idle {} => {
            let idle = idle.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(idle.subscribe());
                Box::new(idle::Idle::start(idle))
            }))
        }
        AppletKind::Session {} => {
            let session_actions = session_actions.clone();
            let dialog = dialog.cloned()?;
            Some(Box::new(move |ctx| {
                ctx.watch(session_actions.subscribe());
                Box::new(session::Session::start(session_actions, dialog))
            }))
        }
        AppletKind::Battery(_) => {
            let battery = battery.clone();
            let notifications = notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(battery.subscribe());
                Box::new(battery::Battery::start(battery, notifications))
            }))
        }
        AppletKind::Clipboard(_) => {
            let clipboard = clipboard.clone();
            let notifications = notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(clipboard.subscribe());
                Box::new(clipboard::Clipboard::start(clipboard, notifications))
            }))
        }
        AppletKind::Places(_) => {
            let places = places.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(places.subscribe());
                Box::new(places::Places::start(places))
            }))
        }
        AppletKind::Printing(_) => {
            let printing = printing.clone();
            let notifications = notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(printing.subscribe());
                Box::new(printing::Printing::start(printing, notifications))
            }))
        }
        AppletKind::Removable(_) => {
            let removable = removable.clone();
            let notifications = notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(removable.subscribe());
                Box::new(removable::Removable::start(removable, notifications))
            }))
        }
        AppletKind::Kdeconnect(_) => {
            let kdeconnect = kdeconnect.clone();
            let notifications = notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(kdeconnect.subscribe());
                Box::new(kdeconnect::Kdeconnect::start(kdeconnect, notifications))
            }))
        }
        AppletKind::Privacy(_) => {
            let privacy = privacy.clone();
            let compositor = compositor.clone();
            let notifications = notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(privacy.subscribe());
                Box::new(privacy::Privacy::start(privacy, compositor, notifications))
            }))
        }
        AppletKind::ColorPicker {} => {
            let color_picker = color_picker.clone();
            let notifications = notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(color_picker.subscribe());
                Box::new(color_picker::ColorPicker::start(
                    color_picker,
                    notifications,
                ))
            }))
        }
        AppletKind::WorkspaceName {} => {
            let compositor = compositor.clone();
            let notifications = notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(compositor.subscribe());
                Box::new(workspace_name::WorkspaceName::start(
                    compositor,
                    notifications,
                ))
            }))
        }
        AppletKind::Command(_) => {
            let notifications = notifications.clone();
            Some(Box::new(move |_ctx| {
                Box::new(command::Command::start(notifications))
            }))
        }
        AppletKind::SystemMonitor(_) => {
            let system_monitor = system_monitor.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(system_monitor.subscribe());
                Box::new(system_monitor::SystemMonitor::start(system_monitor))
            }))
        }
        AppletKind::Ruler {} => {
            let ruler = ruler.clone();
            let notifications = notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(ruler.subscribe());
                Box::new(ruler::Ruler::start(ruler, notifications))
            }))
        }
        AppletKind::Exec {} => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom(name: &str, extends: AppletConfig) -> BTreeMap<String, AppletConfig> {
        BTreeMap::from([(name.to_owned(), extends)])
    }

    #[test]
    fn every_name_in_the_default_zones_resolves_to_a_kind() {
        let panel = glimpse_config::Panel::default();
        for name in panel.left.iter().chain(&panel.center).chain(&panel.right) {
            assert!(
                configured(name, &BTreeMap::new(), &Regional::default()).is_some(),
                "`{name}` ships in a default zone, so an untouched installation must resolve it"
            );
        }
    }

    #[test]
    fn a_known_name_resolves_to_configuration() {
        assert!(configured("heartbeat", &BTreeMap::new(), &Regional::default()).is_some());
    }

    #[test]
    fn extends_names_the_kind_so_one_kind_can_have_several_instances() {
        let applets = custom("pulse", AppletKind::Heartbeat {}.into());
        assert!(
            configured("pulse", &applets, &Regional::default()).is_some(),
            "`pulse` is not a kind; `extends` is what says which one it is"
        );
        assert!(
            configured("pulse", &BTreeMap::new(), &Regional::default()).is_none(),
            "without the entry the same name is just unknown"
        );
    }

    #[test]
    fn the_next_event_applet_is_configured_rather_than_skipped() {
        assert!(
            configured("next-event", &BTreeMap::new(), &Regional::default()).is_some(),
            "the kind is known, so it must not be treated as a typo"
        );
    }

    #[test]
    fn the_tray_applet_is_configured_and_no_longer_falls_through_to_nothing() {
        let applets = configured("tray", &BTreeMap::new(), &Regional::default())
            .expect("`tray` ships in the default right zone, so it must resolve");
        assert!(
            matches!(applets.kind, AppletKind::Tray(_)),
            "a unit variant would let deny_unknown_fields swallow every key under it"
        );
    }

    #[test]
    fn the_weather_applet_is_configured_rather_than_skipped() {
        assert!(
            configured("weather", &BTreeMap::new(), &Regional::default()).is_some(),
            "the kind is known, so it must not be treated as a typo"
        );
    }

    #[test]
    fn the_keyboard_applet_is_configured_rather_than_skipped() {
        assert!(
            configured("keyboard", &BTreeMap::new(), &Regional::default()).is_some(),
            "the kind is known, so it must not be treated as a typo"
        );
    }

    #[test]
    fn the_clipboard_applet_is_configured_and_no_longer_falls_through_to_nothing() {
        let applets = configured("clipboard", &BTreeMap::new(), &Regional::default())
            .expect("`clipboard` ships in the default right zone, so it must resolve");
        assert!(
            matches!(applets.kind, AppletKind::Clipboard(_)),
            "a unit variant would let deny_unknown_fields swallow every key under it"
        );
    }

    #[test]
    fn the_notifications_applet_is_configured_rather_than_skipped() {
        assert!(
            configured("notifications", &BTreeMap::new(), &Regional::default()).is_some(),
            "the kind is known, so it must not be treated as a typo"
        );
    }

    #[test]
    fn a_kind_without_an_implementation_is_not_the_same_as_a_typo() {
        assert!(
            AppletConfig::from_name("removable").is_some(),
            "`removable` is a real applet, so skipping it is expected rather than a bad document"
        );
        assert!(AppletConfig::from_name("nonesuch").is_none());
        assert!(configured("nonesuch", &BTreeMap::new(), &Regional::default()).is_none());
    }

    #[tokio::test]
    async fn the_printing_applet_now_produces_a_builder() {
        let services = crate::services::PanelServices::start_with_buses(
            &glimpse_config::Config::default(),
            glimpse_dbus::Buses::unavailable("no bus in tests"),
        );
        let config: AppletConfig = AppletKind::Printing(<_>::default()).into();

        let built = build(
            &config,
            &services.compositor,
            &services.keyboard,
            &services.calendar,
            &services.mpris,
            &services.heartbeat,
            &services.tray,
            &services.bluetooth,
            &services.network,
            &services.audio,
            &services.brightness,
            &services.night_light(),
            &services.notifications(),
            &services.weather(),
            &services.idle(),
            &services.color_picker,
            &services.session_actions,
            &services.battery,
            &services.clipboard,
            &services.places,
            &services.printing,
            &services.removable,
            &services.kdeconnect,
            &services.privacy,
            &services.system_monitor,
            &services.ruler,
            None,
        );
        assert!(built.is_some(), "printing now has an implementation");

        services.shutdown().await;
    }

    #[tokio::test]
    async fn the_privacy_applet_now_produces_a_builder() {
        let services = crate::services::PanelServices::start_with_buses(
            &glimpse_config::Config::default(),
            glimpse_dbus::Buses::unavailable("no bus in tests"),
        );
        let config: AppletConfig = AppletKind::Privacy(<_>::default()).into();

        let built = build(
            &config,
            &services.compositor,
            &services.keyboard,
            &services.calendar,
            &services.mpris,
            &services.heartbeat,
            &services.tray,
            &services.bluetooth,
            &services.network,
            &services.audio,
            &services.brightness,
            &services.night_light(),
            &services.notifications(),
            &services.weather(),
            &services.idle(),
            &services.color_picker,
            &services.session_actions,
            &services.battery,
            &services.clipboard,
            &services.places,
            &services.printing,
            &services.removable,
            &services.kdeconnect,
            &services.privacy,
            &services.system_monitor,
            &services.ruler,
            None,
        );
        assert!(built.is_some(), "privacy now has an implementation");

        services.shutdown().await;
    }

    #[tokio::test]
    async fn the_clipboard_applet_now_produces_a_builder() {
        let services = crate::services::PanelServices::start_with_buses(
            &glimpse_config::Config::default(),
            glimpse_dbus::Buses::unavailable("no bus in tests"),
        );
        let config: AppletConfig = AppletKind::Clipboard(<_>::default()).into();

        let built = build(
            &config,
            &services.compositor,
            &services.keyboard,
            &services.calendar,
            &services.mpris,
            &services.heartbeat,
            &services.tray,
            &services.bluetooth,
            &services.network,
            &services.audio,
            &services.brightness,
            &services.night_light(),
            &services.notifications(),
            &services.weather(),
            &services.idle(),
            &services.color_picker,
            &services.session_actions,
            &services.battery,
            &services.clipboard,
            &services.places,
            &services.printing,
            &services.removable,
            &services.kdeconnect,
            &services.privacy,
            &services.system_monitor,
            &services.ruler,
            None,
        );
        assert!(built.is_some(), "clipboard now has an implementation");

        services.shutdown().await;
    }

    #[tokio::test]
    async fn the_places_applet_is_built_and_renders_a_chip() {
        let services = crate::services::PanelServices::start_with_buses(
            &glimpse_config::Config::default(),
            glimpse_dbus::Buses::unavailable("no bus in tests"),
        );
        let config = configured("places", &BTreeMap::new(), &Regional::default())
            .expect("`places` is a known applet");

        let built = build(
            &config,
            &services.compositor,
            &services.keyboard,
            &services.calendar,
            &services.mpris,
            &services.heartbeat,
            &services.tray,
            &services.bluetooth,
            &services.network,
            &services.audio,
            &services.brightness,
            &services.night_light(),
            &services.notifications(),
            &services.weather(),
            &services.idle(),
            &services.color_picker,
            &services.session_actions,
            &services.battery,
            &services.clipboard,
            &services.places,
            &services.printing,
            &services.removable,
            &services.kdeconnect,
            &services.privacy,
            &services.system_monitor,
            &services.ruler,
            None,
        );
        assert!(built.is_some(), "places has an implementation");

        services.shutdown().await;
    }

    #[tokio::test]
    async fn the_removable_applet_is_built_from_its_own_name() {
        let services = crate::services::PanelServices::start_with_buses(
            &glimpse_config::Config::default(),
            glimpse_dbus::Buses::unavailable("no bus in tests"),
        );
        let config = configured("removable", &BTreeMap::new(), &Regional::default())
            .expect("`removable` is a known applet");

        let built = build(
            &config,
            &services.compositor,
            &services.keyboard,
            &services.calendar,
            &services.mpris,
            &services.heartbeat,
            &services.tray,
            &services.bluetooth,
            &services.network,
            &services.audio,
            &services.brightness,
            &services.night_light(),
            &services.notifications(),
            &services.weather(),
            &services.idle(),
            &services.color_picker,
            &services.session_actions,
            &services.battery,
            &services.clipboard,
            &services.places,
            &services.printing,
            &services.removable,
            &services.kdeconnect,
            &services.privacy,
            &services.system_monitor,
            &services.ruler,
            None,
        );
        assert!(built.is_some(), "removable has an implementation");

        services.shutdown().await;
    }

    #[tokio::test]
    async fn the_system_monitor_applet_produces_a_builder() {
        let services = crate::services::PanelServices::start_with_buses(
            &glimpse_config::Config::default(),
            glimpse_dbus::Buses::unavailable("no bus in tests"),
        );
        let config = configured("system-monitor", &BTreeMap::new(), &Regional::default())
            .expect("`system-monitor` is a known applet");

        let built = build(
            &config,
            &services.compositor,
            &services.keyboard,
            &services.calendar,
            &services.mpris,
            &services.heartbeat,
            &services.tray,
            &services.bluetooth,
            &services.network,
            &services.audio,
            &services.brightness,
            &services.night_light(),
            &services.notifications(),
            &services.weather(),
            &services.idle(),
            &services.color_picker,
            &services.session_actions,
            &services.battery,
            &services.clipboard,
            &services.places,
            &services.printing,
            &services.removable,
            &services.kdeconnect,
            &services.privacy,
            &services.system_monitor,
            &services.ruler,
            None,
        );
        assert!(built.is_some(), "system-monitor has an implementation");

        services.shutdown().await;
    }

    #[tokio::test]
    async fn the_audio_applet_now_produces_a_builder() {
        let services = crate::services::PanelServices::start_with_buses(
            &glimpse_config::Config::default(),
            glimpse_dbus::Buses::unavailable("no bus in tests"),
        );
        let config: AppletConfig = AppletKind::Audio {}.into();

        let built = build(
            &config,
            &services.compositor,
            &services.keyboard,
            &services.calendar,
            &services.mpris,
            &services.heartbeat,
            &services.tray,
            &services.bluetooth,
            &services.network,
            &services.audio,
            &services.brightness,
            &services.night_light(),
            &services.notifications(),
            &services.weather(),
            &services.idle(),
            &services.color_picker,
            &services.session_actions,
            &services.battery,
            &services.clipboard,
            &services.places,
            &services.printing,
            &services.removable,
            &services.kdeconnect,
            &services.privacy,
            &services.system_monitor,
            &services.ruler,
            None,
        );
        assert!(built.is_some(), "audio now has an implementation");

        services.shutdown().await;
    }

    #[tokio::test]
    async fn the_session_applet_needs_the_dialog_host() {
        let services = crate::services::PanelServices::start_with_buses(
            &glimpse_config::Config::default(),
            glimpse_dbus::Buses::unavailable("no bus in tests"),
        );
        let config: AppletConfig = AppletKind::Session {}.into();
        let (dialog, _rx) = relm4::channel();

        assert!(
            build(
                &config,
                &services.compositor,
                &services.keyboard,
                &services.calendar,
                &services.mpris,
                &services.heartbeat,
                &services.tray,
                &services.bluetooth,
                &services.network,
                &services.audio,
                &services.brightness,
                &services.night_light(),
                &services.notifications(),
                &services.weather(),
                &services.idle(),
                &services.color_picker,
                &services.session_actions,
                &services.battery,
                &services.clipboard,
                &services.places,
                &services.printing,
                &services.removable,
                &services.kdeconnect,
                &services.privacy,
                &services.system_monitor,
                &services.ruler,
                None,
            )
            .is_none(),
            "a session chip cannot confirm without the host"
        );
        assert!(
            build(
                &config,
                &services.compositor,
                &services.keyboard,
                &services.calendar,
                &services.mpris,
                &services.heartbeat,
                &services.tray,
                &services.bluetooth,
                &services.network,
                &services.audio,
                &services.brightness,
                &services.night_light(),
                &services.notifications(),
                &services.weather(),
                &services.idle(),
                &services.color_picker,
                &services.session_actions,
                &services.battery,
                &services.clipboard,
                &services.places,
                &services.printing,
                &services.removable,
                &services.kdeconnect,
                &services.privacy,
                &services.system_monitor,
                &services.ruler,
                Some(&dialog),
            )
            .is_some(),
            "session is a configured panel chip"
        );

        services.shutdown().await;
    }

    #[tokio::test]
    async fn the_brightness_display_and_idle_applets_produce_a_builder() {
        let services = crate::services::PanelServices::start_with_buses(
            &glimpse_config::Config::default(),
            glimpse_dbus::Buses::unavailable("no bus in tests"),
        );
        let brightness_config: AppletConfig = AppletKind::Brightness(<_>::default()).into();
        let display_config: AppletConfig = AppletKind::Display {}.into();
        let idle_config: AppletConfig = AppletKind::Idle {}.into();
        let battery_config: AppletConfig = AppletKind::Battery(<_>::default()).into();
        let command_config: AppletConfig = AppletKind::Command(<_>::default()).into();
        let ruler_config: AppletConfig = AppletKind::Ruler {}.into();

        for config in [
            &brightness_config,
            &display_config,
            &idle_config,
            &battery_config,
            &command_config,
            &ruler_config,
        ] {
            let built = build(
                config,
                &services.compositor,
                &services.keyboard,
                &services.calendar,
                &services.mpris,
                &services.heartbeat,
                &services.tray,
                &services.bluetooth,
                &services.network,
                &services.audio,
                &services.brightness,
                &services.night_light(),
                &services.notifications(),
                &services.weather(),
                &services.idle(),
                &services.color_picker,
                &services.session_actions,
                &services.battery,
                &services.clipboard,
                &services.places,
                &services.printing,
                &services.removable,
                &services.kdeconnect,
                &services.privacy,
                &services.system_monitor,
                &services.ruler,
                None,
            );
            assert!(
                built.is_some(),
                "each configured panel chip must build from one config"
            );
        }

        services.shutdown().await;
    }

    /// The shipped defaults name applets in a panel zone and carry no `[applets.<name>]` table for
    /// them, so every applet on a default bar is built by `from_name` rather than found in the
    /// map. That path invents a config, and an invented config that kept `Regional::default()`
    /// would silently ignore an explicit `hour-format`.
    #[test]
    fn an_applet_with_no_table_of_its_own_still_carries_the_document_s_regional() {
        let twelve = Regional {
            hour_format: glimpse_config::HourFormat::Twelve,
            ..Regional::default()
        };

        let built = configured("clock", &BTreeMap::new(), &twelve).expect("the clock configures");
        assert!(
            built.regional.twelve_hour(),
            "an applet with no table of its own must still read the document's `[regional]`"
        );

        let found = configured(
            "clock",
            &custom("clock", AppletKind::Clock(<_>::default()).into()),
            &twelve,
        )
        .expect("the clock configures");
        assert!(
            found.regional.twelve_hour(),
            "and so must one that has a table"
        );
    }
}
