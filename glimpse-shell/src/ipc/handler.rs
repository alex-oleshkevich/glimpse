use std::pin::Pin;

use glimpse_core::ThemeMode;
use glimpse_core::ipc::client::CommandHandler;
use glimpse_core::services::{
    audio, battery, bluetooth, brightness, clipboard, keyboard, location, mpris, network,
    notifications, power, storage, theme, weather,
};

use crate::services::framework::{ServiceCommand, ServiceHandle, Services};

#[derive(Clone)]
pub(crate) struct ShellCommandHandler {
    pub services: Services,
}

/// Dispatch a command to a service, surfacing a send failure to the IPC caller
/// instead of silently dropping it. A full or closed service channel means the
/// command did not take effect, so the client must see an error rather than a
/// false success.
fn dispatch<S: Clone, C: Send>(
    handle: &ServiceHandle<S, C>,
    service: &'static str,
    command: C,
) -> Result<Vec<(String, String)>, String> {
    match handle.try_send(ServiceCommand::Command(command)) {
        Ok(()) => Ok(Vec::new()),
        Err(error) => {
            tracing::warn!(service, %error, "ipc command dropped: service channel unavailable");
            Err(format!("{service}: {error}"))
        }
    }
}

fn field<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn require<'a>(fields: &'a [(String, String)], key: &str) -> Result<&'a str, String> {
    field(fields, key).ok_or_else(|| format!("missing {key}"))
}

fn parse_bool(v: &str) -> Result<bool, String> {
    match v {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!("expected true or false, got '{other}'")),
    }
}

fn parse_percent(v: &str) -> Result<u32, String> {
    let n: u32 = v.parse().map_err(|_| "must be an integer".to_owned())?;
    if n > 100 {
        return Err("must be between 0 and 100".to_owned());
    }
    Ok(n)
}

/// Destructive commands require an explicit `confirm=true` field — the socket
/// is unauthenticated (0600, same-user) so a stray script shouldn't be able to
/// forget networks / wipe the clipboard / power off a drive by accident.
fn require_confirm(fields: &[(String, String)]) -> Result<(), String> {
    if field(fields, "confirm") == Some("true") {
        Ok(())
    } else {
        Err("destructive command requires confirm=true".to_owned())
    }
}

impl ShellCommandHandler {
    fn status(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        let audio = self.services.audio.snapshot();
        if let Some(dev) = audio.default_output() {
            out.push(("audio_volume".into(), dev.volume.to_string()));
            out.push(("audio_muted".into(), dev.muted.to_string()));
        }
        let net = self.services.network.snapshot();
        out.push((
            "connectivity".into(),
            net.snapshot.status.connectivity.clone(),
        ));
        if let Some(ap) = net.snapshot.wifi_access_points.iter().find(|a| a.connected) {
            out.push(("wifi_ssid".into(), ap.ssid.clone()));
        }
        let bt = self.services.bluetooth.snapshot();
        out.push((
            "bluetooth_powered".into(),
            bt.snapshot.status.powered.to_string(),
        ));
        let battery = self.services.battery.snapshot();
        out.push((
            "battery_percent".into(),
            battery.status.percentage.to_string(),
        ));
        if let Some(src) = self
            .services
            .brightness
            .snapshot()
            .sources
            .iter()
            .find(|s| s.primary)
        {
            out.push(("brightness_percent".into(), src.percent.to_string()));
        }
        out.push((
            "power_profile".into(),
            self.services.power.snapshot().profiles.active.clone(),
        ));
        out.push((
            "dnd".into(),
            self.services.notifications.snapshot().dnd.to_string(),
        ));
        if let Some(p) = self.services.mpris.snapshot().snapshot.current_player {
            out.push(("mpris_player".into(), p.identity));
            out.push(("mpris_status".into(), format!("{:?}", p.playback_status)));
        }
        append_location_status(&mut out, &self.services.location.snapshot());
        append_weather_status(&mut out, &self.services.weather.snapshot());
        out
    }

    fn primary_brightness_id(&self) -> Option<String> {
        let state = self.services.brightness.snapshot();
        state
            .sources
            .iter()
            .find(|s| s.primary)
            .or_else(|| state.sources.first())
            .map(|s| s.id.clone())
    }

    fn current_player_id(&self) -> Option<String> {
        self.services
            .mpris
            .snapshot()
            .snapshot
            .current_player
            .map(|p| p.player_id)
    }
}

fn append_location_status(out: &mut Vec<(String, String)>, state: &location::State) {
    if let location::State::Ready(coordinates) = state {
        out.push(("location_latitude".into(), coordinates.latitude.to_string()));
        out.push((
            "location_longitude".into(),
            coordinates.longitude.to_string(),
        ));
    }
}

fn append_weather_status(out: &mut Vec<(String, String)>, state: &weather::model::State) {
    use weather::model::State as WeatherState;

    match state {
        WeatherState::Ready(snapshot) => {
            out.push(("weather_state".into(), "ready".into()));
            out.push(("weather_icon".into(), snapshot.current.icon.clone()));
            out.push((
                "weather_temperature".into(),
                format!("{:.0}°C", snapshot.current.temperature),
            ));
            out.push((
                "weather_condition".into(),
                snapshot.current.condition.clone(),
            ));
            out.push(("weather_city".into(), snapshot.location.city.clone()));
        }
        WeatherState::Loading => out.push(("weather_state".into(), "loading".into())),
        WeatherState::Unavailable(error) => {
            out.push(("weather_state".into(), "unavailable".into()));
            out.push(("weather_error".into(), error.clone()));
        }
        WeatherState::Unknown => out.push(("weather_state".into(), "unknown".into())),
    }
}

impl CommandHandler for ShellCommandHandler {
    fn execute<'a>(
        &'a self,
        name: &'a str,
        fields: &'a [(String, String)],
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<(String, String)>, String>> + Send + 'a>>
    {
        Box::pin(async move {
            let svc = &self.services;
            match name {
                "status" => Ok(self.status()),

                // ── audio ──────────────────────────────────────────────
                "set_volume" => {
                    let level = parse_percent(require(fields, "level")?)?;
                    dispatch(&svc.audio, "audio", audio::Command::SetOutputVolume(level))
                }
                "toggle_mute" => dispatch(&svc.audio, "audio", audio::Command::ToggleOutputMute),
                "set_input_volume" => {
                    let level = parse_percent(require(fields, "level")?)?;
                    dispatch(&svc.audio, "audio", audio::Command::SetInputVolume(level))
                }
                "toggle_input_mute" => {
                    dispatch(&svc.audio, "audio", audio::Command::ToggleInputMute)
                }

                // ── brightness ─────────────────────────────────────────
                "set_brightness" => {
                    let percent = parse_percent(require(fields, "percent")?)? as u8;
                    let id = match field(fields, "id") {
                        Some(id) => id.to_owned(),
                        None => self
                            .primary_brightness_id()
                            .ok_or("no brightness source available")?,
                    };
                    dispatch(
                        &svc.brightness,
                        "brightness",
                        brightness::Command::SetPercent { id, percent },
                    )
                }
                "adjust_brightness" => {
                    let delta: i32 = require(fields, "delta")?
                        .parse()
                        .map_err(|_| "delta must be an integer".to_owned())?;
                    let id = match field(fields, "id") {
                        Some(id) => id.to_owned(),
                        None => self
                            .primary_brightness_id()
                            .ok_or("no brightness source available")?,
                    };
                    dispatch(
                        &svc.brightness,
                        "brightness",
                        brightness::Command::AdjustPercent { id, delta },
                    )
                }

                // ── power ──────────────────────────────────────────────
                "set_power_profile" => {
                    let profile = require(fields, "profile")?.to_owned();
                    dispatch(&svc.power, "power", power::Command::SetProfile(profile))
                }

                // ── notifications ──────────────────────────────────────
                "set_dnd" => {
                    let enabled = parse_bool(require(fields, "enabled")?)?;
                    dispatch(
                        &svc.notifications,
                        "notifications",
                        notifications::model::Command::SetDnd(enabled),
                    )
                }
                "dismiss_notification" => {
                    let id: u32 = require(fields, "id")?
                        .parse()
                        .map_err(|_| "id must be an integer".to_owned())?;
                    dispatch(
                        &svc.notifications,
                        "notifications",
                        notifications::model::Command::Dismiss { id },
                    )
                }
                "dismiss_all_notifications" => dispatch(
                    &svc.notifications,
                    "notifications",
                    notifications::model::Command::DismissAll,
                ),

                // ── media (mpris) ──────────────────────────────────────
                "media_play_pause" | "media_next" | "media_previous" => {
                    let player_id = match field(fields, "player") {
                        Some(p) => p.to_owned(),
                        None => self.current_player_id().ok_or("no active media player")?,
                    };
                    let cmd = match name {
                        "media_play_pause" => mpris::Command::PlayPause { player_id },
                        "media_next" => mpris::Command::Next { player_id },
                        _ => mpris::Command::Previous { player_id },
                    };
                    dispatch(&svc.mpris, "mpris", cmd)
                }

                // ── theme ──────────────────────────────────────────────
                "set_theme" => {
                    let mode = match require(fields, "mode")? {
                        "light" => ThemeMode::Light,
                        "dark" => ThemeMode::Dark,
                        "auto" => ThemeMode::Auto,
                        other => {
                            return Err(format!(
                                "mode must be light, dark, or auto, got '{other}'"
                            ));
                        }
                    };
                    dispatch(&svc.theme, "theme", theme::Command::SetMode(mode))
                }

                // ── keyboard ───────────────────────────────────────────
                "next_keyboard_layout" => {
                    dispatch(&svc.keyboard, "keyboard", keyboard::Command::NextLayout)
                }
                "prev_keyboard_layout" => dispatch(
                    &svc.keyboard,
                    "keyboard",
                    keyboard::Command::PreviousLayout,
                ),
                "set_keyboard_layout" => {
                    let index: usize = require(fields, "index")?
                        .parse()
                        .map_err(|_| "index must be a non-negative integer".to_owned())?;
                    dispatch(
                        &svc.keyboard,
                        "keyboard",
                        keyboard::Command::SetLayout(index),
                    )
                }

                // ── network ────────────────────────────────────────────
                "set_wifi" => {
                    let enabled = parse_bool(require(fields, "enabled")?)?;
                    dispatch(
                        &svc.network,
                        "network",
                        network::Command::SetWifiEnabled(enabled),
                    )
                }
                "wifi_scan" => dispatch(&svc.network, "network", network::Command::RequestScan),
                "connect_wifi" => {
                    let ssid = require(fields, "ssid")?.to_owned();
                    let path = require(fields, "path")?.to_owned();
                    dispatch(
                        &svc.network,
                        "network",
                        network::Command::ConnectWifi { ssid, path },
                    )
                }
                "forget_wifi" => {
                    require_confirm(fields)?;
                    let uuid = require(fields, "uuid")?.to_owned();
                    dispatch(&svc.network, "network", network::Command::Forget { uuid })
                }

                // ── bluetooth ──────────────────────────────────────────
                "set_bluetooth" => {
                    let enabled = parse_bool(require(fields, "enabled")?)?;
                    dispatch(
                        &svc.bluetooth,
                        "bluetooth",
                        bluetooth::Command::SetPowered(enabled),
                    )
                }
                "bluetooth_scan" => {
                    let cmd = match require(fields, "action")? {
                        "start" => bluetooth::Command::StartDiscovery,
                        "stop" => bluetooth::Command::StopDiscovery,
                        other => {
                            return Err(format!("action must be start or stop, got '{other}'"));
                        }
                    };
                    dispatch(&svc.bluetooth, "bluetooth", cmd)
                }
                "connect_bluetooth" => {
                    let address = require(fields, "address")?.to_owned();
                    dispatch(
                        &svc.bluetooth,
                        "bluetooth",
                        bluetooth::Command::Connect { address },
                    )
                }
                "disconnect_bluetooth" => {
                    let address = require(fields, "address")?.to_owned();
                    dispatch(
                        &svc.bluetooth,
                        "bluetooth",
                        bluetooth::Command::Disconnect { address },
                    )
                }
                "forget_bluetooth" => {
                    require_confirm(fields)?;
                    let address = require(fields, "address")?.to_owned();
                    dispatch(
                        &svc.bluetooth,
                        "bluetooth",
                        bluetooth::Command::Forget { address },
                    )
                }

                // ── clipboard (destructive) ────────────────────────────
                "clear_clipboard" => {
                    require_confirm(fields)?;
                    dispatch(
                        &svc.clipboard,
                        "clipboard",
                        clipboard::Command::ClearClipboard,
                    )
                }
                "clear_clipboard_history" => {
                    require_confirm(fields)?;
                    dispatch(
                        &svc.clipboard,
                        "clipboard",
                        clipboard::Command::ClearHistory,
                    )
                }

                // ── storage (destructive) ──────────────────────────────
                "eject" => {
                    require_confirm(fields)?;
                    let id = require(fields, "id")?.to_owned();
                    dispatch(&svc.storage, "storage", storage::Command::Eject { id })
                }
                "poweroff_drive" => {
                    require_confirm(fields)?;
                    let id = require(fields, "id")?.to_owned();
                    dispatch(&svc.storage, "storage", storage::Command::PowerOff { id })
                }

                // ── refresh ────────────────────────────────────────────
                "refresh" => match require(fields, "service")? {
                    "battery" => dispatch(&svc.battery, "battery", battery::Command::Refresh),
                    "brightness" => {
                        dispatch(&svc.brightness, "brightness", brightness::Command::Refresh)
                    }
                    "power" => dispatch(&svc.power, "power", power::Command::Refresh),
                    "storage" => dispatch(&svc.storage, "storage", storage::Command::Refresh),
                    "location" => dispatch(&svc.location, "location", location::Command::Refresh),
                    other => Err(format!(
                        "unknown service '{other}' (battery|brightness|location|power|storage)"
                    )),
                },

                // ── set_location ──────────────────────────────────────
                // Manual override that bypasses GeoClue — useful for testing,
                // travel scenarios where geolocation is wrong, or kiosk setups.
                "set_location" => {
                    let lat: f64 = require(fields, "lat")?
                        .parse()
                        .map_err(|_| "lat must be a number".to_string())?;
                    let lon: f64 = require(fields, "lon")?
                        .parse()
                        .map_err(|_| "lon must be a number".to_string())?;
                    if !(-90.0..=90.0).contains(&lat) {
                        return Err("lat must be in [-90, 90]".to_string());
                    }
                    if !(-180.0..=180.0).contains(&lon) {
                        return Err("lon must be in [-180, 180]".to_string());
                    }
                    dispatch(
                        &svc.location,
                        "location",
                        location::Command::SetManual(lat, lon),
                    )
                }

                _ => Err(format!("unknown command: {name}")),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{append_location_status, append_weather_status, dispatch};
    use glimpse_core::services::{
        audio,
        framework::ServiceHandle,
        location::{Coordinates, State},
        weather::model::{
            CurrentWeather, Location as WeatherLocation, Snapshot, State as WeatherState,
        },
    };
    use tokio::sync::{mpsc, watch};

    #[test]
    fn dispatch_surfaces_error_when_service_channel_closed() {
        let (_state_tx, state_rx) = watch::channel(audio::State::default());
        let (cmd_tx, cmd_rx) = mpsc::channel(1);
        drop(cmd_rx); // service task gone → sends must fail
        let handle: ServiceHandle<audio::State, audio::Command> =
            ServiceHandle::new(state_rx, cmd_tx);

        let result = dispatch(&handle, "audio", audio::Command::ToggleOutputMute);

        assert!(
            result.is_err(),
            "a closed service channel must surface as an IPC error, not a false success"
        );
    }

    #[test]
    fn dispatch_reports_success_when_command_is_accepted() {
        let (_state_tx, state_rx) = watch::channel(audio::State::default());
        let (cmd_tx, _cmd_rx) = mpsc::channel(1);
        let handle: ServiceHandle<audio::State, audio::Command> =
            ServiceHandle::new(state_rx, cmd_tx);

        let result = dispatch(&handle, "audio", audio::Command::ToggleOutputMute);

        assert_eq!(result, Ok(vec![]));
    }

    #[test]
    fn status_includes_ready_location_coordinates() {
        let mut fields = Vec::new();

        append_location_status(
            &mut fields,
            &State::Ready(Coordinates {
                latitude: 52.2297,
                longitude: 21.0122,
            }),
        );

        assert_eq!(
            fields,
            vec![
                ("location_latitude".into(), "52.2297".into()),
                ("location_longitude".into(), "21.0122".into()),
            ]
        );
    }

    #[test]
    fn status_omits_location_coordinates_when_unknown() {
        let mut fields = Vec::new();

        append_location_status(&mut fields, &State::Unknown);

        assert!(fields.is_empty());
    }

    #[test]
    fn status_includes_ready_weather_summary() {
        let mut fields = Vec::new();

        append_weather_status(
            &mut fields,
            &WeatherState::Ready(Snapshot {
                current: CurrentWeather {
                    temperature: 21.4,
                    condition: "Clear".into(),
                    icon: "weather-clear-symbolic".into(),
                    ..CurrentWeather::default()
                },
                location: WeatherLocation {
                    city: "Warsaw".into(),
                    ..WeatherLocation::default()
                },
                ..Snapshot::default()
            }),
        );

        assert_eq!(
            fields,
            vec![
                ("weather_state".into(), "ready".into()),
                ("weather_icon".into(), "weather-clear-symbolic".into()),
                ("weather_temperature".into(), "21°C".into()),
                ("weather_condition".into(), "Clear".into()),
                ("weather_city".into(), "Warsaw".into()),
            ]
        );
    }
}
