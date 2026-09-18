mod agenda;
pub mod audio;
pub(crate) mod bluetooth;
mod brightness;
mod clock;
mod display;
mod heartbeat;
mod keyboard;
mod mpris;
pub mod network;
mod next_event;
mod notifications;
mod pager;
mod tokens;
mod tray;
pub(crate) mod weather;

use glimpse_config::{Applet as AppletConfig, AppletKind, Regional};
use glimpse_dbus::{
    night_light::NightLightProviderHandle, notifications::NotificationsProviderHandle,
    weather::WeatherProviderHandle,
};
use glimpse_services::{
    AudioHandle, BluetoothHandle, BrightnessHandle, CalendarHandle, CompositorHandle,
    HeartbeatHandle, KeyboardHandle, MprisHandle, NetworkHandle, TrayHandle,
};
use std::collections::BTreeMap;

use crate::applet::runtime::Builder;

pub fn configured(
    name: &str,
    configured: &BTreeMap<String, AppletConfig>,
    regional: &Regional,
) -> Option<AppletConfig> {
    let config = configured
        .get(name)
        .cloned()
        .or_else(|| AppletConfig::from_name(name));
    let Some(mut config) = config else {
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
        AppletKind::Battery {}
        | AppletKind::Clipboard {}
        | AppletKind::Command {}
        | AppletKind::Exec {}
        | AppletKind::Idle {}
        | AppletKind::Privacy {}
        | AppletKind::Printing {}
        | AppletKind::Removable {}
        | AppletKind::Session {} => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom(name: &str, extends: AppletConfig) -> BTreeMap<String, AppletConfig> {
        BTreeMap::from([(name.to_owned(), extends)])
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
    fn the_notifications_applet_is_configured_rather_than_skipped() {
        assert!(
            configured("notifications", &BTreeMap::new(), &Regional::default()).is_some(),
            "the kind is known, so it must not be treated as a typo"
        );
    }

    #[test]
    fn a_kind_without_an_implementation_is_not_the_same_as_a_typo() {
        assert!(
            AppletConfig::from_name("battery").is_some(),
            "`battery` is a real applet, so skipping it is expected rather than a bad document"
        );
        assert!(AppletConfig::from_name("nonesuch").is_none());
        assert!(configured("nonesuch", &BTreeMap::new(), &Regional::default()).is_none());
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
        );
        assert!(built.is_some(), "audio now has an implementation");

        services.shutdown().await;
    }

    #[tokio::test]
    async fn the_brightness_and_display_applets_both_produce_a_builder() {
        let services = crate::services::PanelServices::start_with_buses(
            &glimpse_config::Config::default(),
            glimpse_dbus::Buses::unavailable("no bus in tests"),
        );

        let brightness_config: AppletConfig = AppletKind::Brightness(<_>::default()).into();
        let display_config: AppletConfig = AppletKind::Display {}.into();

        for config in [&brightness_config, &display_config] {
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
            );
            assert!(
                built.is_some(),
                "both AC-17 chips must build from one config"
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
