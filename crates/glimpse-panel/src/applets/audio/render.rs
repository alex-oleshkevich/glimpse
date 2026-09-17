use gettextrs::gettext;
use gio_unix::{DesktopAppInfo, prelude::*};
use glimpse_services::{AUDIO_NAME_CAP, AudioApp, AudioDevice, AudioDirection, AudioState};

const MUTED: &str = "audio-volume-muted-symbolic";
const LOW: &str = "audio-volume-low-symbolic";
const MEDIUM: &str = "audio-volume-medium-symbolic";
const HIGH: &str = "audio-volume-high-symbolic";
const OVERAMPLIFIED: &str = "audio-volume-overamplified-symbolic";
const MIC_MUTED: &str = "microphone-disabled-symbolic";

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
    let status = output_status(output.volume, output.muted);
    let Some(format) = format else {
        return Some(status);
    };
    Some(crate::applets::tokens::render(
        format,
        |token| match token {
            "status" => Some(status.as_str()),
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
        false => cap(&app.name, AUDIO_NAME_CAP),
    }
}

pub fn cap(text: &str, chars: usize) -> String {
    glimpse_utils::clean(text, chars)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_services::{AudioAppId, AudioDeviceId};

    fn device(id: &str, volume: u32, muted: bool, default: bool) -> AudioDevice {
        AudioDevice {
            id: AudioDeviceId::new(id),
            index: 0,
            name: id.to_owned(),
            icon_name: None,
            form_factor: None,
            volume,
            muted,
            default,
        }
    }

    fn output_device(form_factor: Option<&str>, icon_name: Option<&str>) -> AudioDevice {
        AudioDevice {
            id: AudioDeviceId::new("dev"),
            index: 0,
            name: "dev".to_owned(),
            icon_name: icon_name.map(str::to_owned),
            form_factor: form_factor.map(str::to_owned),
            volume: 100,
            muted: false,
            default: true,
        }
    }

    fn state_with_output(volume: u32, muted: bool) -> AudioState {
        AudioState {
            outputs: vec![device("speaker", volume, muted, true)],
            ..AudioState::default()
        }
    }

    fn app(name: &str, icon_name: Option<&str>) -> AudioApp {
        AudioApp {
            id: AudioAppId::new("app"),
            name: name.to_owned(),
            icon_name: icon_name.map(str::to_owned),
            app_id: None,
            binary: None,
            playback: None,
            capture: None,
        }
    }

    #[test]
    fn chip_bands_the_output_volume_at_its_boundaries() {
        assert_eq!(chip(&state_with_output(1, false)), Some(LOW));
        assert_eq!(chip(&state_with_output(32, false)), Some(LOW));
        assert_eq!(chip(&state_with_output(33, false)), Some(MEDIUM));
        assert_eq!(chip(&state_with_output(65, false)), Some(MEDIUM));
        assert_eq!(chip(&state_with_output(66, false)), Some(HIGH));
        assert_eq!(chip(&state_with_output(100, false)), Some(HIGH));
        assert_eq!(chip(&state_with_output(101, false)), Some(OVERAMPLIFIED));
    }

    #[test]
    fn chip_shows_silence_when_muted_at_any_volume() {
        assert_eq!(chip(&state_with_output(80, true)), Some(MUTED));
    }

    #[test]
    fn chip_shows_silence_when_unmuted_at_zero_volume() {
        assert_eq!(chip(&state_with_output(0, false)), Some(MUTED));
    }

    #[test]
    fn tooltip_tells_muted_from_zero_volume() {
        let muted = tooltip(&state_with_output(50, true), None).expect("a tooltip");
        let silent = tooltip(&state_with_output(0, false), None).expect("a tooltip");

        assert_ne!(muted, silent);
        assert_eq!(muted, gettext("Muted"));
        assert_eq!(silent, "0%");
    }

    #[test]
    fn overlay_flags_a_muted_default_input() {
        let state = AudioState {
            inputs: vec![device("mic", 80, true, true)],
            ..AudioState::default()
        };
        assert_eq!(overlay(&state), Some(MIC_MUTED));
    }

    #[test]
    fn overlay_is_none_for_an_unmuted_default_input() {
        let state = AudioState {
            inputs: vec![device("mic", 80, false, true)],
            ..AudioState::default()
        };
        assert_eq!(overlay(&state), None);
    }

    #[test]
    fn an_empty_audio_state_shows_no_chip() {
        assert_eq!(chip(&AudioState::default()), None);
    }

    #[test]
    fn device_icon_prefers_the_form_factor_over_the_icon_name() {
        let device = output_device(Some("headset"), Some("audio-headset-bluetooth"));
        assert_eq!(device_icon(&device, AudioDirection::Output), HEADSET);
    }

    #[test]
    fn device_icon_reads_headset_before_bluetooth_in_the_icon_name() {
        let device = output_device(None, Some("audio-headset-bluetooth"));
        assert_eq!(device_icon(&device, AudioDirection::Output), HEADSET);
    }

    #[test]
    fn device_icon_falls_through_to_the_direction_default() {
        let device = output_device(None, Some("audio-card-analog"));
        assert_eq!(device_icon(&device, AudioDirection::Output), SPEAKERS);
        assert_eq!(device_icon(&device, AudioDirection::Input), MICROPHONE);
    }

    #[test]
    fn an_unidentifiable_app_gets_the_generic_executable_icon() {
        assert_eq!(app_icon(&app("Mystery", None)), APP_FALLBACK);
    }

    #[test]
    fn desktop_icon_cannot_resolve_an_id_nothing_installs() {
        assert_eq!(desktop_icon("nonesuch.invalid"), None);
    }

    #[test]
    fn desktop_icon_rejects_an_id_that_looks_like_a_path() {
        assert_eq!(desktop_icon("../etc/passwd"), None);
    }

    #[test]
    fn desktop_icon_rejects_an_id_carrying_an_interior_nul() {
        assert_eq!(desktop_icon("evil\0name"), None);
    }

    #[test]
    fn app_icon_falls_back_to_the_generic_icon_when_neither_rung_resolves() {
        let mut unresolvable = app("Mystery", None);
        unresolvable.app_id = Some("nonesuch.invalid".to_owned());
        unresolvable.binary = Some("nonesuch-binary".to_owned());
        assert_eq!(app_icon(&unresolvable), APP_FALLBACK);
    }

    const DESKTOP_CANDIDATES: [&str; 2] = ["Alacritty", "htop"];

    #[test]
    fn app_icon_prefers_a_desktop_lookup_by_app_id_when_one_resolves() {
        let Some((id, icon)) = DESKTOP_CANDIDATES
            .iter()
            .find_map(|id| desktop_icon(id).map(|icon| (*id, icon)))
        else {
            eprintln!(
                "skipping app_icon_prefers_a_desktop_lookup_by_app_id_when_one_resolves: \
                 none of {DESKTOP_CANDIDATES:?} is installed on this machine"
            );
            return;
        };

        let mut resolved = app("Mystery", None);
        resolved.app_id = Some(id.to_owned());
        assert_eq!(app_icon(&resolved), icon);
    }

    #[test]
    fn app_icon_falls_back_to_binary_when_app_id_does_not_resolve() {
        let Some((binary, icon)) = DESKTOP_CANDIDATES
            .iter()
            .find_map(|id| desktop_icon(id).map(|icon| (*id, icon)))
        else {
            eprintln!(
                "skipping app_icon_falls_back_to_binary_when_app_id_does_not_resolve: \
                 none of {DESKTOP_CANDIDATES:?} is installed on this machine"
            );
            return;
        };

        let mut resolved = app("Mystery", None);
        resolved.app_id = Some("nonesuch.invalid".to_owned());
        resolved.binary = Some(binary.to_owned());
        assert_eq!(app_icon(&resolved), icon);
    }

    #[test]
    fn an_empty_app_name_renders_the_translated_fallback() {
        assert_eq!(app_name(&app("", None)), gettext("Unknown application"));
    }

    #[test]
    fn a_hostile_app_name_is_capped_before_it_reaches_a_label() {
        let long = "應用程式 ".repeat(20);
        let shown = app_name(&app(&long, None));

        assert!(shown.chars().count() <= AUDIO_NAME_CAP + 1);
        assert!(
            shown.ends_with('…'),
            "a cut name must not read as a whole one"
        );
    }

    #[test]
    fn a_hostile_device_name_is_capped_before_it_reaches_a_label() {
        let long = "Наушники ".repeat(20);
        let shown = cap(&long, AUDIO_NAME_CAP);

        assert!(shown.chars().count() <= AUDIO_NAME_CAP + 1);
        assert!(shown.ends_with('…'));
    }
}
