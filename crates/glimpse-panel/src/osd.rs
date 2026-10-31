use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use glimpse_dbus::osd::OsdProxy;
use glimpse_services::{
    AudioDevice, AudioDirection, AudioState, BrightnessKind, BrightnessState, KeyboardLayouts,
};
use tokio::task::JoinHandle;
use zbus::proxy::CacheProperties;
use zbus::zvariant::Value;

use crate::applets::audio::render::{device_icon, fader_icon};
use crate::services::Handles;

const DISPLAY_ICON: &str = "display-brightness-symbolic";
const KEYBOARD_BACKLIGHT_ICON: &str = "keyboard-brightness-symbolic";
const LAYOUT_ICON: &str = "input-keyboard-symbolic";

#[derive(Debug, Clone, PartialEq)]
pub struct Show {
    pub icon: String,
    pub label: String,
    pub value: Option<f64>,
}

pub fn spawn(handles: &Handles, session: zbus::Connection) -> JoinHandle<()> {
    let mut audio = handles.audio.subscribe();
    let mut brightness = handles.brightness.subscribe();
    let mut keyboard = handles.keyboard.subscribe();
    let quiet = handles.osd_quiet.clone();
    relm4::spawn(async move {
        let proxy = match OsdProxy::builder(&session)
            .cache_properties(CacheProperties::No)
            .build()
            .await
        {
            Ok(proxy) => proxy,
            Err(err) => {
                tracing::debug!("cannot reach the on-screen display: {err}");
                return;
            }
        };
        let mut audio_seen = audio.borrow_and_update().clone();
        let mut brightness_seen = brightness.borrow_and_update().clone();
        let mut keyboard_seen = keyboard.borrow_and_update().clone();
        loop {
            let shows = tokio::select! {
                changed = audio.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let current = audio.borrow_and_update().clone();
                    let shows = audio_shows(&audio_seen, &current);
                    audio_seen = current;
                    shows
                }
                changed = brightness.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let current = brightness.borrow_and_update().clone();
                    let shows = brightness_shows(&brightness_seen, &current);
                    brightness_seen = current;
                    shows
                }
                changed = keyboard.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let current = keyboard.borrow_and_update().clone();
                    let shows = keyboard_shows(&keyboard_seen, &current);
                    keyboard_seen = current;
                    shows
                }
            };
            for show in quieted(shows, &quiet) {
                send(&proxy, show).await;
            }
        }
    })
}

pub fn quieted(shows: Vec<Show>, quiet: &Arc<AtomicUsize>) -> Vec<Show> {
    if quiet.load(Ordering::Relaxed) == 0 {
        return shows;
    }
    shows
        .into_iter()
        .filter(|show| show.value.is_none())
        .collect()
}

async fn send(proxy: &OsdProxy<'_>, show: Show) {
    let mut values: HashMap<&str, Value<'_>> = HashMap::new();
    if !show.icon.is_empty() {
        values.insert("icon", Value::from(show.icon.as_str()));
    }
    if !show.label.is_empty() {
        values.insert("label", Value::from(show.label.as_str()));
    }
    if let Some(value) = show.value {
        values.insert("value", Value::from(value));
    }
    match tokio::time::timeout(glimpse_dbus::DEADLINE, proxy.show(values)).await {
        Ok(Ok(())) => {}
        Ok(Err(err)) => tracing::debug!("the on-screen display refused a change: {err}"),
        Err(_) => tracing::debug!("the on-screen display did not answer"),
    }
}

pub fn audio_shows(previous: &AudioState, current: &AudioState) -> Vec<Show> {
    audio_show(
        previous.default_output(),
        current.default_output(),
        AudioDirection::Output,
    )
    .or_else(|| {
        audio_show(
            previous.default_input(),
            current.default_input(),
            AudioDirection::Input,
        )
    })
    .into_iter()
    .collect()
}

fn audio_show(
    previous: Option<&AudioDevice>,
    current: Option<&AudioDevice>,
    direction: AudioDirection,
) -> Option<Show> {
    let (previous, current) = (previous?, current?);
    if previous.id != current.id || previous.port != current.port {
        return Some(Show {
            icon: device_icon(current, direction).to_owned(),
            label: current.port.clone().unwrap_or_else(|| current.name.clone()),
            value: None,
        });
    }
    if previous.volume == current.volume && previous.muted == current.muted {
        return None;
    }
    Some(Show {
        icon: fader_icon(direction, current.volume, current.muted).to_owned(),
        label: String::new(),
        value: Some(if current.muted {
            0.0
        } else {
            (current.volume as f64 / 100.0).min(2.0)
        }),
    })
}

pub fn brightness_shows(previous: &BrightnessState, current: &BrightnessState) -> Vec<Show> {
    current
        .sources
        .iter()
        .find_map(|source| {
            let before = previous.sources.iter().find(|old| old.id == source.id)?;
            if before.current == source.current || source.max == 0 {
                return None;
            }
            let icon = match source.kind {
                BrightnessKind::Display => DISPLAY_ICON,
                BrightnessKind::Keyboard => KEYBOARD_BACKLIGHT_ICON,
            };
            Some(Show {
                icon: icon.to_owned(),
                label: String::new(),
                value: Some(source.current as f64 / source.max as f64),
            })
        })
        .into_iter()
        .collect()
}

pub fn keyboard_shows(previous: &KeyboardLayouts, current: &KeyboardLayouts) -> Vec<Show> {
    if previous.current.is_none()
        || previous.layouts.is_empty()
        || previous.current == current.current
        || current.restored
        || current.layouts.len() < 2
    {
        return Vec::new();
    }
    let Some(layout) = current
        .current
        .and_then(|index| current.layouts.get(usize::from(index)))
    else {
        return Vec::new();
    };
    vec![Show {
        icon: LAYOUT_ICON.to_owned(),
        label: layout.name.clone(),
        value: None,
    }]
}

#[cfg(test)]
mod tests;
