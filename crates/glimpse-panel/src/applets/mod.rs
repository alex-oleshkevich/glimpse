mod agenda;
pub mod audio;
pub(crate) mod bluetooth;
pub(crate) mod clipboard;
mod clock;
mod command;
mod display;
pub(crate) mod exec;
mod heartbeat;
pub(crate) mod idle;
mod keyboard;
mod mpris;
pub mod network;
mod next_event;
mod notifications;
mod pager;
mod places;
mod power;
mod printing;
mod privacy;
mod removable;
mod session;
mod tokens;
mod tray;
pub(crate) mod weather;
mod widget_demo;
mod workspace;
mod workspace_name;

use glimpse_config::{Applet as AppletConfig, AppletKind, Regional};
use std::collections::BTreeMap;

use crate::applet::runtime::Builder;

pub fn configured(
    name: &str,
    configured: &BTreeMap<String, AppletConfig>,
    regional: &Regional,
) -> Option<AppletConfig> {
    let mut config = glimpse_config::resolve_applet(name, configured)?;
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

pub fn build(
    config: &AppletConfig,
    handles: &crate::services::Handles,
    placement: glimpse_services::Placement,
    dialog: Option<&relm4::Sender<crate::app::AppInput>>,
) -> Option<Builder> {
    match &config.kind {
        AppletKind::Clock(_) => {
            let calendar = handles.calendar.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(calendar.subscribe());
                Box::new(clock::Clock::start(calendar))
            }))
        }
        AppletKind::Tray(_) => {
            let tray = handles.tray.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(tray.subscribe());
                Box::new(tray::Tray::start(tray, placement))
            }))
        }
        AppletKind::Heartbeat {} => {
            let heartbeat = handles.heartbeat.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(heartbeat.subscribe());
                Box::new(heartbeat::Heartbeat::start(heartbeat))
            }))
        }
        AppletKind::WidgetDemo {} => Some(Box::new(|_ctx| Box::new(widget_demo::WidgetDemo))),
        AppletKind::Mpris(_) => {
            let mpris = handles.mpris.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(mpris.subscribe());
                Box::new(mpris::Mpris::start(mpris))
            }))
        }
        AppletKind::NextEvent(_) => {
            let calendar = handles.calendar.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(calendar.subscribe());
                Box::new(next_event::NextEvent::start(calendar))
            }))
        }
        AppletKind::Pager(_) => {
            let compositor = handles.compositor.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(compositor.subscribe());
                Box::new(pager::Pager::start(compositor))
            }))
        }
        AppletKind::Weather(_) => {
            let weather = handles.weather.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(weather.subscribe());
                Box::new(weather::Weather::start(weather))
            }))
        }
        AppletKind::Keyboard { .. } => {
            let keyboard = handles.keyboard.clone();
            let notifications = handles.notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(keyboard.subscribe());
                Box::new(keyboard::Keyboard::start(keyboard, notifications))
            }))
        }
        AppletKind::Notifications(_) => {
            let notifications = handles.notifications.clone();
            let compositor = handles.compositor.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(notifications.subscribe());
                Box::new(notifications::Notifications::start(
                    notifications,
                    compositor,
                ))
            }))
        }
        AppletKind::Bluetooth(_) => {
            let bluetooth = handles.bluetooth.clone();
            let notifications = handles.notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(bluetooth.subscribe());
                Box::new(bluetooth::Bluetooth::start(bluetooth, notifications))
            }))
        }
        AppletKind::Network(_) => {
            let network = handles.network.clone();
            let notifications = handles.notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(network.subscribe());
                Box::new(network::Network::start(network, notifications))
            }))
        }
        AppletKind::Audio(_) => {
            let audio = handles.audio.clone();
            let notifications = handles.notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(audio.subscribe());
                Box::new(audio::Audio::start(audio, notifications))
            }))
        }
        AppletKind::Display(_) => {
            let brightness = handles.brightness.clone();
            let night_light = handles.night_light.clone();
            let compositor = handles.compositor.clone();
            let notifications = handles.notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(brightness.subscribe());
                ctx.watch(night_light.subscribe());
                ctx.watch(compositor.subscribe());
                Box::new(display::Display::start(
                    brightness,
                    night_light,
                    compositor,
                    notifications,
                ))
            }))
        }
        AppletKind::Idle { .. } => {
            let idle = handles.idle.clone();
            let notifications = handles.notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(idle.subscribe());
                Box::new(idle::Idle::start(idle, notifications))
            }))
        }
        AppletKind::Session { .. } => {
            let session_actions = handles.session_actions.clone();
            let dialog = dialog.cloned()?;
            Some(Box::new(move |ctx| {
                ctx.watch(session_actions.subscribe());
                Box::new(session::Session::start(session_actions, dialog))
            }))
        }
        AppletKind::Power(_) => {
            let battery = handles.battery.clone();
            let idle = handles.idle.clone();
            let notifications = handles.notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(battery.subscribe());
                ctx.watch(idle.subscribe());
                Box::new(power::Power::start(battery, idle, notifications))
            }))
        }
        AppletKind::Clipboard(_) => {
            let clipboard = handles.clipboard.clone();
            let kdeconnect = handles.kdeconnect.clone();
            let notifications = handles.notifications.clone();
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
            let places = handles.places.clone();
            let notifications = handles.notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(places.subscribe());
                Box::new(places::Places::start(places, notifications))
            }))
        }
        AppletKind::Printing(_) => {
            let printing = handles.printing.clone();
            let notifications = handles.notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(printing.subscribe());
                Box::new(printing::Printing::start(printing, notifications))
            }))
        }
        AppletKind::Removable(_) => {
            let removable = handles.removable.clone();
            let notifications = handles.notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(removable.subscribe());
                Box::new(removable::Removable::start(removable, notifications))
            }))
        }
        AppletKind::Privacy(_) => {
            let privacy = handles.privacy.clone();
            let compositor = handles.compositor.clone();
            let audio = handles.audio.clone();
            let notifications = handles.notifications.clone();
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
        AppletKind::WorkspaceName { .. } => {
            let compositor = handles.compositor.clone();
            let notifications = handles.notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(compositor.subscribe());
                Box::new(workspace_name::WorkspaceName::start(
                    compositor,
                    notifications,
                ))
            }))
        }
        AppletKind::Command(_) => {
            let notifications = handles.notifications.clone();
            Some(Box::new(move |_ctx| {
                Box::new(command::Command::start(notifications))
            }))
        }
        AppletKind::Exec(_) => {
            let exec = handles.exec.clone();
            let notifications = handles.notifications.clone();
            Some(Box::new(move |ctx| {
                ctx.watch(exec.subscribe());
                Box::new(exec::Exec::start(
                    ctx.name(),
                    exec,
                    notifications,
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
