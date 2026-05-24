use std::{pin::Pin, sync::Arc};

use tokio::sync::broadcast;

use glimpse_core::{
    Config, DAYLIGHT_TEMPERATURE_KELVIN, NightLightHealth, NightLightPhase, NightLightSchedule,
    ipc::{self, IpcHandle, IpcServer, client::CommandHandler, sunset_socket_path},
    services::{
        night_light::{self, NightLightHandle, State},
        solar::{self, SolarHandle},
    },
};

use crate::shell_location;

pub fn start(night_light: NightLightHandle, solar: SolarHandle) -> IpcHandle {
    let tx = ipc::new_event_channel();
    spawn_watcher(night_light.subscribe(), tx.clone());
    IpcServer::launch_at(
        tx,
        sunset_socket_path(),
        SunsetCommandHandler { night_light, solar },
    )
}

fn spawn_watcher(
    mut rx: tokio::sync::watch::Receiver<State>,
    tx: broadcast::Sender<Arc<glimpse_core::ipc::protocol::IpcEvent>>,
) {
    tokio::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            if prev.phase != next.phase {
                ipc::emit(
                    &tx,
                    "nightlight.phase_changed",
                    vec![("phase", phase_name(next.phase).to_owned())],
                );
                if next.phase == NightLightPhase::Night {
                    ipc::emit(
                        &tx,
                        "nightlight.activated",
                        vec![("temperature", next.effective_temperature_kelvin.to_string())],
                    );
                } else if next.phase == NightLightPhase::Day {
                    ipc::emit(&tx, "nightlight.deactivated", vec![]);
                }
            }

            if prev.effective_temperature_kelvin != next.effective_temperature_kelvin {
                ipc::emit(
                    &tx,
                    "nightlight.temperature_changed",
                    vec![
                        ("kelvin", next.effective_temperature_kelvin.to_string()),
                        ("phase", phase_name(next.phase).to_owned()),
                    ],
                );
            }

            if prev.health != next.health {
                ipc::emit(
                    &tx,
                    "nightlight.health_changed",
                    vec![("health", health_name(&next.health).to_owned())],
                );
            }

            prev = next;
        }
    });
}

fn phase_name(phase: NightLightPhase) -> &'static str {
    match phase {
        NightLightPhase::Disabled => "disabled",
        NightLightPhase::Day => "day",
        NightLightPhase::TransitionToNight => "transition_to_night",
        NightLightPhase::Night => "night",
        NightLightPhase::TransitionToDay => "transition_to_day",
    }
}

fn health_name(health: &NightLightHealth) -> &'static str {
    match health {
        NightLightHealth::Starting => "starting",
        NightLightHealth::Ready => "ready",
        NightLightHealth::Unsupported => "unsupported",
        NightLightHealth::Reconnecting { .. } => "reconnecting",
        NightLightHealth::Degraded { .. } => "degraded",
    }
}

fn schedule_name(schedule: NightLightSchedule) -> &'static str {
    match schedule {
        NightLightSchedule::Off => "off",
        NightLightSchedule::Automatic => "automatic",
        NightLightSchedule::Schedule => "schedule",
    }
}

fn parse_schedule(s: &str) -> Result<NightLightSchedule, String> {
    match s {
        "off" => Ok(NightLightSchedule::Off),
        "automatic" => Ok(NightLightSchedule::Automatic),
        "schedule" => Ok(NightLightSchedule::Schedule),
        other => Err(format!(
            "unknown schedule '{other}': expected off, automatic, or schedule"
        )),
    }
}

#[derive(Clone)]
struct SunsetCommandHandler {
    night_light: NightLightHandle,
    solar: SolarHandle,
}

impl CommandHandler for SunsetCommandHandler {
    fn execute<'a>(
        &'a self,
        name: &'a str,
        fields: &'a [(String, String)],
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<(String, String)>, String>> + Send + 'a>>
    {
        Box::pin(async move {
            let get = |key: &str| {
                fields
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.as_str())
            };

            match name {
                "refresh" => {
                    shell_location::refresh_shell_location();
                    self.solar.try_send_command(
                        "solar",
                        solar::Command::Refresh,
                        "failed to send solar refresh",
                    );
                    self.night_light.try_send_command(
                        "night-light",
                        night_light::Command::Refresh,
                        "failed to send night-light refresh",
                    );
                    Ok(vec![])
                }

                "status" => {
                    let state = self.night_light.snapshot();
                    let mut fields = vec![
                        ("phase".into(), phase_name(state.phase).into()),
                        (
                            "kelvin".into(),
                            state.effective_temperature_kelvin.to_string(),
                        ),
                        (
                            "target_kelvin".into(),
                            state.target_temperature_kelvin.to_string(),
                        ),
                        (
                            "schedule".into(),
                            schedule_name(state.config.schedule).into(),
                        ),
                        ("health".into(), health_name(&state.health).into()),
                    ];
                    if let Some(forced) = state.manual_override {
                        fields.push(("manual".into(), if forced { "night" } else { "day" }.into()));
                    }
                    Ok(fields)
                }

                "solar" => match self.solar.snapshot() {
                    solar::State::Ready(snapshot) => Ok(vec![
                        ("state".into(), "ready".into()),
                        ("sunrise".into(), snapshot.times.sunrise.clone()),
                        ("sunset".into(), snapshot.times.sunset.clone()),
                        ("date".into(), snapshot.date.to_string()),
                    ]),
                    solar::State::Unknown => Ok(vec![("state".into(), "unknown".into())]),
                    solar::State::Degraded { .. } => Ok(vec![("state".into(), "degraded".into())]),
                },

                "activate" => {
                    self.night_light.try_send_command(
                        "night-light",
                        night_light::Command::Manual(true),
                        "failed to activate night light",
                    );
                    Ok(vec![])
                }

                "enable" => {
                    // Clear any manual override first so the automatic schedule takes effect.
                    self.night_light.try_send_command(
                        "night-light",
                        night_light::Command::Manual(false),
                        "failed to clear manual override",
                    );
                    let mut config = self.night_light.snapshot().config;
                    config.schedule = NightLightSchedule::Automatic;
                    self.night_light.try_send_command(
                        "night-light",
                        night_light::Command::ApplyConfig(config),
                        "failed to enable night light",
                    );
                    Ok(vec![])
                }

                "disable" => {
                    // Clear the manual override too, otherwise a later set_schedule
                    // would resurrect forced Night because ApplyConfig preserves it.
                    self.night_light.try_send_command(
                        "night-light",
                        night_light::Command::Manual(false),
                        "failed to clear manual override",
                    );
                    let mut config = self.night_light.snapshot().config;
                    config.schedule = NightLightSchedule::Off;
                    self.night_light.try_send_command(
                        "night-light",
                        night_light::Command::ApplyConfig(config),
                        "failed to disable night light",
                    );
                    Ok(vec![])
                }

                "set_temperature" => {
                    let kelvin: u32 = get("kelvin")
                        .ok_or("missing kelvin")?
                        .parse()
                        .map_err(|_| "kelvin must be a positive integer")?;
                    if !(1000..=DAYLIGHT_TEMPERATURE_KELVIN).contains(&kelvin) {
                        return Err(format!(
                            "kelvin must be between 1000 and {DAYLIGHT_TEMPERATURE_KELVIN}"
                        ));
                    }
                    let mut config = self.night_light.snapshot().config;
                    config.temperature = kelvin;
                    self.night_light.try_send_command(
                        "night-light",
                        night_light::Command::ApplyConfig(config),
                        "failed to set temperature",
                    );
                    Ok(vec![])
                }

                "set_schedule" => {
                    let schedule = parse_schedule(get("schedule").ok_or("missing schedule")?)?;
                    let mut config = self.night_light.snapshot().config;
                    config.schedule = schedule;
                    self.night_light.try_send_command(
                        "night-light",
                        night_light::Command::ApplyConfig(config),
                        "failed to set schedule",
                    );
                    Ok(vec![])
                }

                "set_times" => {
                    let start = get("start").ok_or("missing start")?;
                    let end = get("end").ok_or("missing end")?;
                    let mut config = self.night_light.snapshot().config;
                    config.start_time = Some(start.to_owned());
                    config.end_time = Some(end.to_owned());
                    // A manual window only takes effect in Schedule mode; switch to it
                    // so the command has a visible effect instead of silently no-op'ing.
                    config.schedule = NightLightSchedule::Schedule;
                    self.night_light.try_send_command(
                        "night-light",
                        night_light::Command::ApplyConfig(config),
                        "failed to set times",
                    );
                    Ok(vec![])
                }

                "set_location" => {
                    let (lat, lon) = parse_location_fields(fields)?;
                    shell_location::set_shell_location(lat, lon)
                        .await
                        .map_err(|error| format!("failed to set shell location: {error}"))?;
                    Ok(vec![])
                }

                "reset" => {
                    self.night_light.try_send_command(
                        "night-light",
                        night_light::Command::Manual(false),
                        "failed to deactivate night light",
                    );
                    let default_temperature =
                        tokio::task::spawn_blocking(|| Config::load().night_light.temperature)
                            .await
                            .map_err(|e| format!("failed to load config: {e}"))?;
                    let mut config = self.night_light.snapshot().config;
                    config.temperature = default_temperature;
                    self.night_light.try_send_command(
                        "night-light",
                        night_light::Command::ApplyConfig(config),
                        "failed to reset temperature",
                    );
                    Ok(vec![])
                }

                _ => Err(format!("unknown command: {name}")),
            }
        })
    }
}

fn parse_location_fields(fields: &[(String, String)]) -> Result<(f64, f64), String> {
    let get = |key: &str| {
        fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    let lat: f64 = get("lat")
        .ok_or("missing lat")?
        .parse()
        .map_err(|_| "lat must be a float")?;
    let lon: f64 = get("lon")
        .ok_or("missing lon")?
        .parse()
        .map_err(|_| "lon must be a float")?;
    if !(-90.0..=90.0).contains(&lat) {
        return Err("lat must be between -90 and 90".into());
    }
    if !(-180.0..=180.0).contains(&lon) {
        return Err("lon must be between -180 and 180".into());
    }
    Ok((lat, lon))
}

#[cfg(test)]
mod tests {
    use super::parse_location_fields;

    #[test]
    fn parse_location_fields_accepts_valid_coordinates() {
        assert_eq!(
            parse_location_fields(&[
                ("lat".into(), "52.2297".into()),
                ("lon".into(), "21.0122".into())
            ]),
            Ok((52.2297, 21.0122))
        );
    }

    #[test]
    fn parse_location_fields_rejects_invalid_coordinates() {
        assert_eq!(
            parse_location_fields(&[
                ("lat".into(), "91".into()),
                ("lon".into(), "21.0122".into())
            ]),
            Err("lat must be between -90 and 90".into())
        );
    }
}
