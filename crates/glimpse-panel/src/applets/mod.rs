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
pub(crate) mod exec;
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
mod widget_demo;
mod workspace;
mod workspace_name;

use glimpse_config::{Applet as AppletConfig, AppletKind, Regional};
use glimpse_dbus::{
    idle::IdleProviderHandle, night_light::NightLightProviderHandle,
    notifications::NotificationsProviderHandle, weather::WeatherProviderHandle,
};
use glimpse_services::{
    AudioHandle, BatteryHandle, BluetoothHandle, BrightnessHandle, CalendarHandle, ClipboardHandle,
    ColorPickerHandle, CompositorHandle, ExecHandle, HeartbeatHandle, KdeconnectHandle,
    KeyboardHandle, MprisHandle, NetworkHandle, PlacesHandle, PrintingHandle, PrivacyHandle,
    RemovableHandle, RulerHandle, SessionActionsHandle, SystemMonitorHandle, TrayHandle,
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

pub(crate) fn output_name(connector: &str, outputs: &[glimpse_services::OutputInfo]) -> String {
    match outputs.iter().find(|output| output.connector == connector) {
        Some(output) if output.built_in => gettextrs::gettext("Built-in display"),
        Some(output) => output.label.clone().unwrap_or_else(|| connector.to_owned()),
        None => connector.to_owned(),
    }
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
    exec: &ExecHandle,
    placement: glimpse_services::Placement,
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
        AppletKind::WidgetDemo {} => Some(Box::new(|_ctx| Box::new(widget_demo::WidgetDemo))),
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
            let kdeconnect = kdeconnect.clone();
            let notifications = notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(clipboard.subscribe());
                ctx.watch(kdeconnect.subscribe());
                Box::new(clipboard::Clipboard::start(
                    clipboard,
                    kdeconnect,
                    notifications,
                ))
            }))
        }
        AppletKind::Places(_) => {
            let places = places.clone();
            let notifications = notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(places.subscribe());
                Box::new(places::Places::start(places, notifications))
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
            let audio = audio.clone();
            let notifications = notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(privacy.subscribe());
                ctx.watch(audio.subscribe());
                Box::new(privacy::Privacy::start(
                    privacy,
                    compositor,
                    audio,
                    notifications,
                ))
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
        AppletKind::Exec(_) => {
            let exec = exec.clone();
            let session_actions = session_actions.clone();
            let notifications = notifications.clone();
            let dialog = dialog.cloned()?;
            Some(Box::new(move |ctx| {
                ctx.watch(exec.subscribe());
                Box::new(exec::Exec::start(
                    ctx.name(),
                    exec,
                    session_actions,
                    notifications,
                    dialog,
                    placement,
                ))
            }))
        }
    }
}

pub(crate) fn app_label(id: &str) -> String {
    use gio_unix::prelude::*;
    if id.is_empty() || id.contains('/') || id.contains('\0') || !id.contains('.') {
        return id.to_owned();
    }
    gio_unix::DesktopAppInfo::new(&format!("{id}.desktop"))
        .map(|app| app.display_name().to_string())
        .unwrap_or_else(|| id.to_owned())
}

#[cfg(test)]
mod tests;
