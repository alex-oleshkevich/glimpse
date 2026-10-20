use gettextrs::gettext;
use gio_unix::{DesktopAppInfo, prelude::*};
use glimpse_services::{AUDIO_NAME_CAP, AudioApp, AudioDevice, AudioDirection, AudioState};

const MUTED: &str = "audio-volume-muted-symbolic";
const LOW: &str = "audio-volume-low-symbolic";
const MEDIUM: &str = "audio-volume-medium-symbolic";
const HIGH: &str = "audio-volume-high-symbolic";
const OVERAMPLIFIED: &str = "audio-volume-overamplified-symbolic";
const MIC_MUTED: &str = "microphone-disabled-symbolic";
const MIC_SILENT: &str = "microphone-sensitivity-muted-symbolic";
const MIC_LOW: &str = "microphone-sensitivity-low-symbolic";
const MIC_MEDIUM: &str = "microphone-sensitivity-medium-symbolic";
const MIC_HIGH: &str = "microphone-sensitivity-high-symbolic";

const HEADSET: &str = "audio-headset-symbolic";
const HEADPHONES: &str = "audio-headphones-symbolic";
const SPEAKERS: &str = "audio-speakers-symbolic";
const MICROPHONE: &str = "audio-input-microphone-symbolic";
const PHONE: &str = "phone-symbolic";
const DISPLAY: &str = "video-display-symbolic";
const BLUETOOTH: &str = "bluetooth-active-symbolic";

const APP_FALLBACK: &str = "application-x-executable-symbolic";

pub fn chip(state: &AudioState) -> Option<&'static str> {
    let output = state.default_output()?;
    Some(level_icon(output.volume, output.muted))
}

/// The glyph a fader carries: a speaker's levels for playback, a microphone's for capture, and the
/// muted glyph of either the moment it is muted, so muting reads as a different icon rather than
/// only a colour.
pub fn fader_icon(direction: AudioDirection, volume: u32, muted: bool) -> &'static str {
    match direction {
        AudioDirection::Output => level_icon(volume, muted),
        AudioDirection::Input if muted || volume == 0 => MIC_SILENT,
        AudioDirection::Input => match volume {
            1..=32 => MIC_LOW,
            33..=65 => MIC_MEDIUM,
            _ => MIC_HIGH,
        },
    }
}

pub fn level_icon(volume: u32, muted: bool) -> &'static str {
    if muted || volume == 0 {
        return MUTED;
    }
    match volume {
        1..=32 => LOW,
        33..=65 => MEDIUM,
        66..=100 => HIGH,
        _ => OVERAMPLIFIED,
    }
}

pub fn overlay(state: &AudioState) -> Option<&'static str> {
    state
        .default_input()
        .filter(|input| input.muted)
        .map(|_| MIC_MUTED)
}

pub fn tooltip(state: &AudioState, format: Option<&str>) -> Option<String> {
    let output = state.default_output()?;
    let volume = output_status(output.volume, output.muted);
    let Some(format) = format else {
        return Some(volume);
    };
    Some(crate::applets::tokens::render(
        format,
        |token| match token {
            "volume" => Some(volume.as_str()),
            _ => None,
        },
    ))
}

pub fn output_status(volume: u32, muted: bool) -> String {
    if muted {
        return gettext("Muted");
    }
    gettext("{percent}%").replace("{percent}", &volume.to_string())
}

pub fn device_icon(device: &AudioDevice, dir: AudioDirection) -> &'static str {
    if let Some(icon) = device.form_factor.as_deref().and_then(form_factor_icon) {
        return icon;
    }

    if let Some(icon_name) = device.icon_name.as_deref() {
        if icon_name.contains("headset") {
            return HEADSET;
        }
        if icon_name.contains("headphone") {
            return HEADPHONES;
        }
        if icon_name.contains("hdmi") || icon_name.contains("video") {
            return DISPLAY;
        }
        if icon_name.contains("bluetooth") {
            return BLUETOOTH;
        }
    }

    match dir {
        AudioDirection::Input => MICROPHONE,
        AudioDirection::Output => SPEAKERS,
    }
}

fn form_factor_icon(form_factor: &str) -> Option<&'static str> {
    Some(match form_factor {
        "headset" => HEADSET,
        "headphone" | "headphones" => HEADPHONES,
        "speaker" => SPEAKERS,
        "microphone" => MICROPHONE,
        "handset" | "phone" => PHONE,
        _ => return None,
    })
}

pub fn app_icon(app: &AudioApp) -> String {
    app.icon_name
        .as_deref()
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .or_else(|| app.app_id.as_deref().and_then(desktop_icon))
        .or_else(|| app.binary.as_deref().and_then(desktop_icon))
        .unwrap_or_else(|| APP_FALLBACK.to_owned())
}

fn desktop_icon(id: &str) -> Option<String> {
    if id.is_empty() || id.contains('/') || id.contains('\0') {
        return None;
    }
    let desktop_id = match id.ends_with(".desktop") {
        true => id.to_owned(),
        false => format!("{id}.desktop"),
    };
    DesktopAppInfo::new(&desktop_id)?
        .icon()?
        .to_string()
        .map(|name| name.to_string())
}

pub(crate) fn app_name(app: &AudioApp) -> String {
    match app.name.is_empty() {
        true => gettext("Unknown application"),
        false => glimpse_utils::clean(&app.name, AUDIO_NAME_CAP),
    }
}

pub(crate) fn heading(output: Option<&AudioDevice>) -> Option<String> {
    output.map(|device| glimpse_utils::clean(&device.name, AUDIO_NAME_CAP))
}

#[cfg(test)]
mod tests;
