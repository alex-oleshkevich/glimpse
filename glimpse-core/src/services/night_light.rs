use std::{error::Error, time::Duration};

use crate::{
    DAYLIGHT_TEMPERATURE_KELVIN, NightLightConfig, NightLightPhase, NightLightSchedule,
    compositors::CompositorType,
    services::{
        compositor,
        framework::{Control, ServiceCommand, ServiceHandle},
        solar,
    },
};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use self::scheduler::{
    ManualScheduleWindow, SolarScheduleWindow, evaluate_automatic_schedule,
    evaluate_manual_schedule, interpolate_temperature,
};

mod scheduler;

const REFRESH_INTERVAL: Duration = Duration::from_secs(60);
const APPLY_TRANSITION_DURATION: Duration = Duration::from_millis(1500);
const APPLY_TRANSITION_STEP: Duration = Duration::from_millis(100);
const COMMAND_QUEUE_SIZE: usize = 8;
const SOLAR_UNAVAILABLE_MESSAGE: &str = "solar times are unavailable for automatic night light";

type ServiceError = Box<dyn Error + Send + Sync>;
type ServiceResult<T> = Result<T, ServiceError>;

#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub compositor: CompositorType,
    pub config: NightLightConfig,
    pub phase: NightLightPhase,
    pub manual_override: Option<bool>,
    pub current_temperature_kelvin: u32,
    pub target_temperature_kelvin: u32,
    pub effective_temperature_kelvin: u32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            compositor: CompositorType::Unsupported,
            config: NightLightConfig::default(),
            phase: NightLightPhase::Disabled,
            manual_override: None,
            current_temperature_kelvin: DAYLIGHT_TEMPERATURE_KELVIN,
            target_temperature_kelvin: DAYLIGHT_TEMPERATURE_KELVIN,
            effective_temperature_kelvin: DAYLIGHT_TEMPERATURE_KELVIN,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Refresh,
    ApplyConfig(NightLightConfig),
    /// Temporarily override the scheduler: true = force Night, false = clear the override.
    /// Cleared by Control::Start/Reconfigure or by Manual(false).
    Manual(bool),
}

pub type NightLightHandle = ServiceHandle<State, Command>;

pub struct NightLightService {
    solar: solar::SolarHandle,
    compositor: compositor::CompositorHandle,
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
    manual_override: Option<bool>,
}

impl NightLightService {
    pub fn new(
        solar: solar::SolarHandle,
        compositor: compositor::CompositorHandle,
    ) -> (Self, NightLightHandle) {
        let (state_tx, state_rx) = watch::channel(initial_state(&compositor.snapshot()));
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);

        (
            Self {
                solar,
                compositor,
                state_tx,
                command_rx,
                manual_override: None,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        tracing::debug!("night light service started");
        let mut interval = tokio::time::interval(REFRESH_INTERVAL);
        let mut solar_rx = self.solar.subscribe();
        let mut compositor_rx = self.compositor.subscribe();
        let mut last_compositor_state = compositor_rx.borrow_and_update().clone();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    self.shutdown().await;
                    break;
                }
                _ = interval.tick() => {
                    self.refresh_from_current_state().await;
                }
                changed = solar_rx.changed() => {
                    if changed.is_err() {
                        tracing::warn!("night light service: solar service subscription closed");
                        break;
                    }
                    self.refresh_from_current_state().await;
                }
                changed = compositor_rx.changed() => {
                    if changed.is_err() {
                        tracing::warn!("night light service: compositor service subscription closed");
                        break;
                    }
                    let next_compositor_state = compositor_rx.borrow_and_update().clone();
                    if compositor_night_light_changed(&last_compositor_state, &next_compositor_state) {
                        self.refresh_from_current_state().await;
                    }
                    last_compositor_state = next_compositor_state;
                }
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Control(Control::Start(config)))
                    | Some(ServiceCommand::Control(Control::Reconfigure(config))) => {
                        self.manual_override = None;
                        self.apply_config(config.night_light).await;
                    }
                    Some(ServiceCommand::Control(Control::Shutdown)) | None => {
                        self.shutdown().await;
                        break;
                    }
                    Some(ServiceCommand::Command(Command::Refresh)) => {
                        self.refresh_from_current_state().await;
                    }
                    Some(ServiceCommand::Command(Command::ApplyConfig(config))) => {
                        // Preserve manual_override across parameter changes (e.g. set_temperature
                        // while in forced Night should stay in Night with the new temperature),
                        // but drop it when the schedule mode itself changes: selecting a schedule
                        // is a request for the schedule to drive again.
                        let schedule_changed =
                            self.state_tx.borrow().config.schedule != config.schedule;
                        if schedule_changed {
                            self.manual_override = None;
                        }
                        self.apply_config(config).await;
                    }
                    Some(ServiceCommand::Command(Command::Manual(forced))) => {
                        self.manual_override = if forced { Some(true) } else { None };
                        self.refresh_from_current_state().await;
                    }
                }
            }
        }

        tracing::debug!("night light service quit");
    }

    async fn refresh_from_current_state(&mut self) {
        let config = self.state_tx.borrow().config.clone();
        self.apply_config(config).await;
    }

    async fn apply_config(&mut self, config: NightLightConfig) {
        if let Err(error) = apply_config(
            &self.solar,
            &self.compositor,
            &self.state_tx,
            config.clone(),
            self.manual_override,
        )
        .await
        {
            let error_message = error.to_string();
            if error_message == SOLAR_UNAVAILABLE_MESSAGE {
                tracing::debug!("night light service: waiting for solar times");
            } else {
                tracing::warn!(error = %error, "night light service: apply failed");
            }
            self.state_tx.send_if_modified(|state| {
                if state.config == config {
                    false
                } else {
                    state.config = config;
                    true
                }
            });
        }
    }

    async fn shutdown(&mut self) {
        if self.state_tx.borrow().effective_temperature_kelvin != DAYLIGHT_TEMPERATURE_KELVIN
            && let Err(error) = reset_night_light(&self.compositor).await
        {
            tracing::debug!(%error, "night light service: failed to reset compositor during shutdown");
        }
    }
}

fn initial_state(compositor_state: &compositor::State) -> State {
    State {
        compositor: compositor_state.compositor,
        ..State::default()
    }
}

fn compositor_night_light_changed(previous: &compositor::State, next: &compositor::State) -> bool {
    previous.compositor != next.compositor
        || previous.capabilities.night_light != next.capabilities.night_light
}

fn service_error(message: impl Into<String>) -> ServiceError {
    Box::new(std::io::Error::other(message.into()))
}

async fn apply_config(
    solar: &solar::SolarHandle,
    compositor: &compositor::CompositorHandle,
    state_tx: &watch::Sender<State>,
    config: NightLightConfig,
    manual_override: Option<bool>,
) -> ServiceResult<()> {
    let compositor_state = compositor.snapshot();
    let compositor_type = compositor_state.compositor;
    let previous_state = state_tx.borrow().clone();

    if !compositor_state.capabilities.night_light {
        let mut next_state = previous_state;
        next_state.compositor = compositor_type;
        next_state.config = config;
        next_state.phase = NightLightPhase::Disabled;
        next_state.manual_override = manual_override;
        next_state.current_temperature_kelvin = DAYLIGHT_TEMPERATURE_KELVIN;
        next_state.target_temperature_kelvin = DAYLIGHT_TEMPERATURE_KELVIN;
        next_state.effective_temperature_kelvin = DAYLIGHT_TEMPERATURE_KELVIN;
        publish_state_if_changed(state_tx, next_state);
        return Ok(());
    }

    let (phase, effective_temperature) =
        match resolve_effective_temperature(&config, solar, manual_override).await {
            Ok(effective) => effective,
            Err(error) if error.to_string() == SOLAR_UNAVAILABLE_MESSAGE => {
                tracing::debug!("night light service: waiting for solar times; using daylight");
                (NightLightPhase::Day, DAYLIGHT_TEMPERATURE_KELVIN)
            }
            Err(error) => return Err(error),
        };

    apply_temperature_transition(
        compositor,
        previous_state.effective_temperature_kelvin,
        effective_temperature,
    )
    .await?;

    log_state_transition(&previous_state, &config, phase, effective_temperature);

    let target_temperature = config.temperature;
    let next_state = State {
        compositor: compositor_type,
        config,
        phase,
        manual_override,
        current_temperature_kelvin: effective_temperature,
        target_temperature_kelvin: target_temperature,
        effective_temperature_kelvin: effective_temperature,
    };
    publish_state_if_changed(state_tx, next_state);

    Ok(())
}

async fn resolve_effective_temperature(
    config: &NightLightConfig,
    solar: &solar::SolarHandle,
    manual_override: Option<bool>,
) -> ServiceResult<(NightLightPhase, u32)> {
    // schedule=Off is an explicit hard disable; it beats the manual override.
    if config.schedule == NightLightSchedule::Off {
        return Ok((NightLightPhase::Disabled, DAYLIGHT_TEMPERATURE_KELVIN));
    }
    if manual_override == Some(true) {
        return Ok((NightLightPhase::Night, config.temperature));
    }
    match config.schedule {
        NightLightSchedule::Off => unreachable!("handled above"),
        NightLightSchedule::Schedule => {
            let start = config
                .start_time
                .as_deref()
                .ok_or_else(|| service_error("scheduled night light start_time is missing"))?;
            let end = config
                .end_time
                .as_deref()
                .ok_or_else(|| service_error("scheduled night light end_time is missing"))?;
            let now = current_local_time();
            let window = ManualScheduleWindow::new(start, end, config.transition_minutes)
                .map_err(service_error)?;
            let evaluation = evaluate_manual_schedule(&window, &now).map_err(service_error)?;
            let effective = interpolate_temperature(
                DAYLIGHT_TEMPERATURE_KELVIN,
                config.temperature,
                evaluation.night_progress,
            );
            tracing::debug!(
                start,
                end,
                now,
                transition_minutes = config.transition_minutes,
                phase = ?evaluation.phase,
                night_progress = evaluation.night_progress,
                effective_temperature_kelvin = effective,
                "night light service: manual schedule evaluated"
            );
            Ok((evaluation.phase, effective))
        }
        NightLightSchedule::Automatic => {
            let snapshot = resolve_solar_snapshot(solar)?;
            let window = SolarScheduleWindow::new(
                &snapshot.times.sunset,
                &snapshot.times.sunrise,
                config.transition_minutes,
            )
            .map_err(service_error)?;
            let now = current_local_time();
            let evaluation = evaluate_automatic_schedule(&window, &now).map_err(service_error)?;
            let effective = interpolate_temperature(
                DAYLIGHT_TEMPERATURE_KELVIN,
                config.temperature,
                evaluation.night_progress,
            );
            tracing::debug!(
                latitude = snapshot.coordinates.latitude,
                longitude = snapshot.coordinates.longitude,
                sunrise = %snapshot.times.sunrise,
                sunset = %snapshot.times.sunset,
                now,
                transition_minutes = config.transition_minutes,
                phase = ?evaluation.phase,
                night_progress = evaluation.night_progress,
                effective_temperature_kelvin = effective,
                "night light service: automatic schedule evaluated"
            );
            Ok((evaluation.phase, effective))
        }
    }
}

fn publish_state_if_changed(state_tx: &watch::Sender<State>, next_state: State) -> bool {
    if *state_tx.borrow() == next_state {
        false
    } else {
        state_tx.send_replace(next_state);
        true
    }
}

fn resolve_solar_snapshot(solar: &solar::SolarHandle) -> ServiceResult<solar::Snapshot> {
    match solar.snapshot() {
        solar::State::Ready(snapshot) => {
            tracing::debug!(
                latitude = snapshot.coordinates.latitude,
                longitude = snapshot.coordinates.longitude,
                sunrise = %snapshot.times.sunrise,
                sunset = %snapshot.times.sunset,
                "night light service: using solar service times"
            );
            Ok(snapshot)
        }
        solar::State::Unknown | solar::State::Degraded { .. } => {
            Err(service_error(SOLAR_UNAVAILABLE_MESSAGE))
        }
    }
}

fn log_state_transition(
    previous_state: &State,
    config: &NightLightConfig,
    phase: NightLightPhase,
    effective_temperature: u32,
) {
    let was_active = previous_state.effective_temperature_kelvin != DAYLIGHT_TEMPERATURE_KELVIN;
    let is_active = effective_temperature != DAYLIGHT_TEMPERATURE_KELVIN;

    if !was_active && is_active {
        tracing::info!(
            schedule = ?config.schedule,
            phase = ?phase,
            target_temperature_kelvin = config.temperature,
            effective_temperature_kelvin = effective_temperature,
            "night light service: activated"
        );
    } else if was_active && !is_active {
        tracing::info!(
            schedule = ?config.schedule,
            previous_phase = ?previous_state.phase,
            phase = ?phase,
            "night light service: deactivated"
        );
    } else if previous_state.phase == phase
        && previous_state.effective_temperature_kelvin == effective_temperature
    {
        tracing::debug!(
            schedule = ?config.schedule,
            phase = ?phase,
            target_temperature_kelvin = config.temperature,
            effective_temperature_kelvin = effective_temperature,
            "night light service: state unchanged after refresh"
        );
    }
}

fn transition_temperatures(from: u32, to: u32) -> Vec<u32> {
    if from == to {
        return Vec::new();
    }

    let step_count = ((APPLY_TRANSITION_DURATION.as_millis() + APPLY_TRANSITION_STEP.as_millis()
        - 1)
        / APPLY_TRANSITION_STEP.as_millis())
    .max(1) as u32;
    let mut temperatures = Vec::with_capacity(step_count as usize);

    for step in 1..=step_count {
        let temperature = interpolate_temperature(from, to, step as f32 / step_count as f32);
        if temperatures.last().copied() != Some(temperature) {
            temperatures.push(temperature);
        }
    }

    temperatures
}

async fn apply_temperature_transition(
    compositor: &compositor::CompositorHandle,
    from: u32,
    to: u32,
) -> ServiceResult<()> {
    let temperatures = transition_temperatures(from, to);

    if temperatures.is_empty() {
        if to == DAYLIGHT_TEMPERATURE_KELVIN {
            apply_temperature_now(compositor, to).await?;
        }
        return Ok(());
    }

    let mut previous_temperature = from;
    for (index, temperature) in temperatures.iter().copied().enumerate() {
        apply_temperature_now(compositor, temperature).await?;
        tracing::debug!(
            previous_temperature_kelvin = previous_temperature,
            effective_temperature_kelvin = temperature,
            "night light service: temperature changed"
        );
        previous_temperature = temperature;

        if index + 1 < temperatures.len() {
            tokio::time::sleep(APPLY_TRANSITION_STEP).await;
        }
    }

    Ok(())
}

async fn apply_temperature_now(
    compositor: &compositor::CompositorHandle,
    effective_temperature: u32,
) -> ServiceResult<()> {
    if effective_temperature == DAYLIGHT_TEMPERATURE_KELVIN {
        reset_night_light(compositor).await?;
    } else {
        compositor
            .send(ServiceCommand::Command(
                compositor::Command::SetNightLightTemperature(effective_temperature),
            ))
            .await
            .map_err(|error| service_error(error.to_string()))?;
    }

    Ok(())
}

async fn reset_night_light(compositor: &compositor::CompositorHandle) -> ServiceResult<()> {
    compositor
        .send(ServiceCommand::Command(
            compositor::Command::ResetNightLight,
        ))
        .await
        .map_err(|error| service_error(error.to_string()))?;
    Ok(())
}

fn current_local_time() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        SOLAR_UNAVAILABLE_MESSAGE, State, apply_config, compositor_night_light_changed,
        resolve_solar_snapshot, transition_temperatures,
    };
    use crate::{
        Config, DAYLIGHT_TEMPERATURE_KELVIN, NightLightConfig, NightLightPhase, NightLightSchedule,
        compositors::{CompositorCapabilities, CompositorType},
        services::{
            compositor::{Command as CompositorCommand, State as CompositorState},
            framework::{Control, ServiceCommand, ServiceHandle},
            solar,
        },
    };
    use tokio::sync::{mpsc, watch};

    fn solar_handle(initial: solar::State) -> (watch::Sender<solar::State>, solar::SolarHandle) {
        let (state_tx, state_rx) = watch::channel(initial);
        let (command_tx, _command_rx) = mpsc::channel(4);
        (state_tx, ServiceHandle::new(state_rx, command_tx))
    }

    fn compositor_handle(
        capabilities: CompositorCapabilities,
    ) -> (
        watch::Sender<CompositorState>,
        crate::services::compositor::CompositorHandle,
        mpsc::Receiver<ServiceCommand<CompositorCommand>>,
    ) {
        let (state_tx, state_rx) = watch::channel(CompositorState {
            compositor: CompositorType::Niri,
            capabilities,
            ..CompositorState::default()
        });
        let (command_tx, command_rx) = mpsc::channel(32);
        (
            state_tx,
            ServiceHandle::new(state_rx, command_tx),
            command_rx,
        )
    }

    #[test]
    fn compositor_updates_only_matter_when_night_light_capability_or_type_changes() {
        let mut prev = CompositorState {
            compositor: CompositorType::Niri,
            capabilities: CompositorCapabilities {
                night_light: true,
                ..CompositorCapabilities::default()
            },
            ..CompositorState::default()
        };
        let mut next = prev.clone();
        next.focused_window = Some(42);

        assert!(!compositor_night_light_changed(&prev, &next));

        next.capabilities.night_light = false;
        assert!(compositor_night_light_changed(&prev, &next));

        prev.capabilities.night_light = false;
        next.compositor = CompositorType::Hyprland;
        assert!(compositor_night_light_changed(&prev, &next));
    }

    async fn next_compositor_command(
        command_rx: &mut mpsc::Receiver<ServiceCommand<CompositorCommand>>,
    ) -> CompositorCommand {
        let command =
            tokio::time::timeout(std::time::Duration::from_millis(250), command_rx.recv())
                .await
                .expect("compositor command should be sent")
                .expect("compositor command channel should stay open");
        match command {
            ServiceCommand::Command(command) => command,
            ServiceCommand::Control(_) => panic!("expected compositor command"),
        }
    }

    #[test]
    fn transition_temperatures_use_multiple_steps_for_large_changes() {
        let steps = transition_temperatures(6500, 4200);

        assert!(steps.len() > 1);
        assert_eq!(steps.last().copied(), Some(4200));
        assert!(steps[0] < 6500);
    }

    #[tokio::test]
    async fn solar_service_times_are_used_for_automatic_schedule() {
        let (_solar_tx, solar) = solar_handle(solar::State::Ready(solar::Snapshot {
            coordinates: crate::services::location::Coordinates {
                latitude: 40.7128,
                longitude: -74.006,
            },
            date: chrono::Local::now().date_naive(),
            times: solar::SolarTimes {
                sunrise: "06:00".into(),
                sunset: "18:00".into(),
            },
        }));

        let snapshot = resolve_solar_snapshot(&solar).expect("solar snapshot");

        assert_eq!(snapshot.times.sunrise, "06:00");
        assert_eq!(snapshot.times.sunset, "18:00");
    }

    #[tokio::test]
    async fn unavailable_solar_times_waits_for_solar_service_update() {
        let (_solar_tx, solar) = solar_handle(solar::State::Unknown);

        let error = resolve_solar_snapshot(&solar).expect_err("solar times should be unavailable");

        assert_eq!(error.to_string(), SOLAR_UNAVAILABLE_MESSAGE);
    }

    #[tokio::test]
    async fn automatic_schedule_resets_stale_night_state_when_solar_is_unavailable() {
        let (_solar_tx, solar) = solar_handle(solar::State::Unknown);
        let (_compositor_tx, compositor, mut compositor_rx) =
            compositor_handle(CompositorCapabilities {
                night_light: true,
                ..CompositorCapabilities::default()
            });
        let config = NightLightConfig {
            schedule: NightLightSchedule::Automatic,
            temperature: 4200,
            ..NightLightConfig::default()
        };
        let (state_tx, _state_rx) = watch::channel(State {
            compositor: CompositorType::Niri,
            config: config.clone(),
            phase: NightLightPhase::Night,
            manual_override: None,
            current_temperature_kelvin: 4200,
            target_temperature_kelvin: 4200,
            effective_temperature_kelvin: 4200,
        });

        apply_config(&solar, &compositor, &state_tx, config, None)
            .await
            .expect("unavailable solar data should fall back to daylight");

        let state = state_tx.borrow().clone();
        assert_eq!(state.phase, NightLightPhase::Day);
        assert_eq!(
            state.effective_temperature_kelvin,
            DAYLIGHT_TEMPERATURE_KELVIN
        );
        assert_eq!(state.target_temperature_kelvin, 4200);

        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if matches!(
                    next_compositor_command(&mut compositor_rx).await,
                    CompositorCommand::ResetNightLight
                ) {
                    break;
                }
            }
        })
        .await
        .expect("daylight fallback should reset the compositor");
    }

    #[tokio::test]
    async fn service_respects_disabled_start_control_config() {
        let (_solar_tx, solar) = solar_handle(solar::State::Ready(solar::Snapshot {
            coordinates: crate::services::location::Coordinates {
                latitude: 52.2298,
                longitude: 21.0118,
            },
            date: chrono::Local::now().date_naive(),
            times: solar::SolarTimes {
                sunrise: "06:00".into(),
                sunset: "18:00".into(),
            },
        }));
        let (_compositor_tx, compositor, mut compositor_rx) =
            compositor_handle(CompositorCapabilities {
                night_light: true,
                ..CompositorCapabilities::default()
            });
        let (service, handle) = super::NightLightService::new(solar, compositor);
        let config = Config {
            night_light: NightLightConfig {
                schedule: NightLightSchedule::Off,
                ..NightLightConfig::default()
            },
            ..Config::default()
        };

        handle
            .send(ServiceCommand::Control(Control::Start(config)))
            .await
            .expect("send start");
        let cancel = tokio_util::sync::CancellationToken::new();
        let service_cancel = cancel.clone();
        let task = tokio::spawn(async move {
            service.run(service_cancel).await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        cancel.cancel();
        let _ = task.await;

        let state = handle.snapshot();
        assert_eq!(state.phase, NightLightPhase::Disabled);
        assert!(matches!(
            next_compositor_command(&mut compositor_rx).await,
            CompositorCommand::ResetNightLight
        ));
    }

    #[tokio::test]
    async fn unsupported_compositor_capability_disables_without_dispatching() {
        let (_solar_tx, solar) = solar_handle(solar::State::Ready(solar::Snapshot {
            coordinates: crate::services::location::Coordinates {
                latitude: 52.2298,
                longitude: 21.0118,
            },
            date: chrono::Local::now().date_naive(),
            times: solar::SolarTimes {
                sunrise: "06:00".into(),
                sunset: "18:00".into(),
            },
        }));
        let (_compositor_tx, compositor, mut compositor_rx) =
            compositor_handle(CompositorCapabilities::default());
        let _compositor_keepalive = compositor.clone();
        let (service, handle) = super::NightLightService::new(solar, compositor);

        handle
            .send(ServiceCommand::Control(Control::Start(Config::default())))
            .await
            .expect("send start");
        let cancel = tokio_util::sync::CancellationToken::new();
        let service_cancel = cancel.clone();
        let task = tokio::spawn(async move {
            service.run(service_cancel).await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        cancel.cancel();
        let _ = task.await;

        let state = handle.snapshot();
        assert_eq!(state.phase, NightLightPhase::Disabled);
        assert_eq!(state.effective_temperature_kelvin, 6500);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), compositor_rx.recv())
                .await
                .is_err(),
            "unsupported compositor should not receive night light commands"
        );
    }

    /// Drains compositor commands so gamma transitions cannot fill the channel
    /// and stall the service loop under test.
    fn drain_compositor_commands(
        mut command_rx: mpsc::Receiver<ServiceCommand<CompositorCommand>>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move { while command_rx.recv().await.is_some() {} })
    }

    async fn wait_for_state(
        handle: &super::NightLightHandle,
        label: &str,
        predicate: impl Fn(&State) -> bool,
    ) -> State {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let state = handle.snapshot();
                if predicate(&state) {
                    return state;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("night light state never satisfied: {label}"))
    }

    /// A running service forced into Night, with the upstream watch senders held
    /// alive — dropping either one closes a subscription and stops the run loop.
    struct ForcedNightService {
        handle: super::NightLightHandle,
        cancel: tokio_util::sync::CancellationToken,
        task: tokio::task::JoinHandle<()>,
        _solar_tx: watch::Sender<solar::State>,
        _compositor_tx: watch::Sender<CompositorState>,
    }

    impl ForcedNightService {
        async fn shutdown(self) {
            self.cancel.cancel();
            let _ = self.task.await;
        }
    }

    /// Starts the service with `Automatic` and forces night via a manual override.
    async fn forced_night_service(temperature: u32) -> ForcedNightService {
        let (solar_tx, solar) = solar_handle(solar::State::Ready(solar::Snapshot {
            coordinates: crate::services::location::Coordinates {
                latitude: 52.2298,
                longitude: 21.0118,
            },
            date: chrono::Local::now().date_naive(),
            times: solar::SolarTimes {
                sunrise: "06:00".into(),
                sunset: "18:00".into(),
            },
        }));
        let (compositor_tx, compositor, compositor_rx) =
            compositor_handle(CompositorCapabilities {
                night_light: true,
                ..CompositorCapabilities::default()
            });
        drain_compositor_commands(compositor_rx);
        let (service, handle) = super::NightLightService::new(solar, compositor);

        let cancel = tokio_util::sync::CancellationToken::new();
        let service_cancel = cancel.clone();
        let task = tokio::spawn(async move { service.run(service_cancel).await });

        handle
            .send(ServiceCommand::Control(Control::Start(Config {
                night_light: NightLightConfig {
                    schedule: NightLightSchedule::Automatic,
                    temperature,
                    ..NightLightConfig::default()
                },
                ..Config::default()
            })))
            .await
            .expect("send start");
        handle
            .send(ServiceCommand::Command(super::Command::Manual(true)))
            .await
            .expect("send manual override");
        wait_for_state(&handle, "forced night", |state| {
            state.manual_override == Some(true)
        })
        .await;

        ForcedNightService {
            handle,
            cancel,
            task,
            _solar_tx: solar_tx,
            _compositor_tx: compositor_tx,
        }
    }

    #[tokio::test]
    async fn changing_the_schedule_clears_a_forced_night_override() {
        let service = forced_night_service(3700).await;

        // Re-selecting a schedule mode is a request for the schedule to drive again.
        service
            .handle
            .send(ServiceCommand::Command(super::Command::ApplyConfig(
                NightLightConfig {
                    schedule: NightLightSchedule::Schedule,
                    temperature: 3700,
                    start_time: Some("18:00".into()),
                    end_time: Some("06:00".into()),
                    ..NightLightConfig::default()
                },
            )))
            .await
            .expect("send apply config");

        let state = wait_for_state(&service.handle, "override cleared", |state| {
            state.manual_override.is_none()
        })
        .await;
        assert_eq!(state.config.schedule, NightLightSchedule::Schedule);

        service.shutdown().await;
    }

    #[tokio::test]
    async fn changing_only_the_temperature_keeps_a_forced_night_override() {
        let service = forced_night_service(3700).await;

        // A parameter change must not silently hand control back to the schedule.
        service
            .handle
            .send(ServiceCommand::Command(super::Command::ApplyConfig(
                NightLightConfig {
                    schedule: NightLightSchedule::Automatic,
                    temperature: 3000,
                    ..NightLightConfig::default()
                },
            )))
            .await
            .expect("send apply config");

        let state = wait_for_state(&service.handle, "temperature applied", |state| {
            state.config.temperature == 3000
        })
        .await;
        assert_eq!(state.manual_override, Some(true));
        assert_eq!(state.phase, NightLightPhase::Night);
        assert_eq!(state.effective_temperature_kelvin, 3000);

        service.shutdown().await;
    }

    #[test]
    fn default_state_starts_at_daylight() {
        let state = State::default();

        assert_eq!(state.current_temperature_kelvin, 6500);
        assert_eq!(state.effective_temperature_kelvin, 6500);
    }
}
