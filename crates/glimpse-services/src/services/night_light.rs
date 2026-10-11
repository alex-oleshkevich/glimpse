use crate::services::{SolarPhase, SolarStatus};
use chrono::{DateTime, Local, NaiveTime, TimeDelta, Utc};
pub use glimpse_config::MAX_TEMPERATURE as DAY;
use glimpse_config::{MIN_TEMPERATURE, Schedule};
use tokio::{
    sync::{oneshot, watch},
    time,
};

use crate::{
    context::Ctx,
    gamma::Gamma,
    publisher::Publisher,
    service::{CommandError, Input, Service, ServiceEndpoint},
    subscription::Sub,
};

const TICK: time::Duration = time::Duration::from_secs(60);
const RAMPING: time::Duration = time::Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    schedule: Schedule,
    temperature: u32,
    start: Option<NaiveTime>,
    end: Option<NaiveTime>,
    transition: TimeDelta,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        let night_light = &document.night_light;
        Self {
            schedule: night_light.schedule,
            temperature: night_light.temperature,
            start: clock(night_light.start_time.as_deref()),
            end: clock(night_light.end_time.as_deref()),
            transition: TimeDelta::minutes(night_light.transition_minutes.into()),
        }
    }
}

fn clock(raw: Option<&str>) -> Option<NaiveTime> {
    glimpse_config::parse_clock(raw?).ok()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NightLightState {
    pub schedule: Schedule,
    pub overridden: bool,
    pub temperature: u32,
    pub target: u32,
    pub configured: Schedule,
    pub manual: bool,
}

impl NightLightState {
    /// Derived rather than stored: two fields that must agree are two chances to disagree.
    pub fn active(&self) -> bool {
        self.temperature != DAY
    }
}

fn initial_state(config: &Config) -> NightLightState {
    NightLightState {
        schedule: config.schedule,
        overridden: false,
        temperature: DAY,
        target: config.temperature,
        configured: config.schedule,
        manual: false,
    }
}

pub enum Event {
    Solar(Option<SolarStatus>),
    Tick,
}

pub enum Command {
    SetSchedule {
        schedule: Schedule,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    SetTemperature {
        kelvin: Option<u32>,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
}

#[derive(PartialEq, Eq, Hash)]
pub enum Watch {
    Solar,
    Tick(time::Duration),
}

pub struct Dependencies {
    pub solar: watch::Receiver<Option<SolarStatus>>,
    pub gamma: Box<dyn Gamma>,
}

pub type NightLightHandle = ServiceEndpoint<NightLight>;

impl ServiceEndpoint<NightLight> {
    pub async fn set_schedule(&self, schedule: Schedule) -> Result<(), CommandError> {
        self.call(|reply| Command::SetSchedule { schedule, reply })
            .await?
    }

    pub async fn set_temperature(&self, kelvin: Option<u32>) -> Result<(), CommandError> {
        self.call(|reply| Command::SetTemperature { kelvin, reply })
            .await?
    }
}

pub struct NightLight {
    state: Publisher<NightLightState>,
    solar: watch::Receiver<Option<SolarStatus>>,
    gamma: Box<dyn Gamma>,
    config: Config,
    forced: Option<Schedule>,
    manual: Option<(u32, SolarPhase)>,
    observed: Option<SolarStatus>,
    applied: Option<u32>,
}

impl Service for NightLight {
    const NAME: &'static str = "night-light";

    type Config = Config;
    type State = NightLightState;
    type Command = Command;
    type Event = Event;
    type Dependencies = Dependencies;
    type SubKey = Watch;

    fn command_stopped_reason(command: &Self::Command) -> &'static str {
        match command {
            Command::SetSchedule { .. } => "night light stopped before accepting the mode",
            Command::SetTemperature { .. } => {
                "night light stopped before accepting the temperature"
            }
        }
    }

    fn initial_state(config: &Self::Config) -> Self::State {
        initial_state(config)
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        if self.effective() == Schedule::Off {
            return Vec::new();
        }
        let cadence = self.cadence(Utc::now());
        let mut declared = vec![Sub::interval(Watch::Tick(cadence), cadence, |_ctx| async {
            Event::Tick
        })];
        if self.effective() == Schedule::Automatic {
            declared.push(Sub::watch(
                Watch::Solar,
                self.solar.clone(),
                Event::Solar,
                Event::Solar(None),
            ));
        }
        declared
    }

    async fn start(
        ctx: &Ctx<Self>,
        config: Self::Config,
        dependencies: Self::Dependencies,
    ) -> Self {
        let observed = dependencies.solar.borrow().clone();
        Self {
            state: ctx.publisher(),
            solar: dependencies.solar,
            gamma: dependencies.gamma,
            config,
            forced: None,
            manual: None,
            observed,
            applied: None,
        }
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        let mut pending = None;
        match input {
            Input::Event(Event::Solar(observed)) => self.observed = observed,
            Input::Event(Event::Tick) => {}
            Input::Command(Command::SetSchedule { schedule, reply }) => {
                self.forced = Some(schedule);
                self.manual = None;
                pending = Some(reply);
            }
            Input::Command(Command::SetTemperature { kelvin, reply }) => match kelvin {
                Some(kelvin) if !(MIN_TEMPERATURE..=DAY).contains(&kelvin) => {
                    let _ = reply.send(Err(CommandError::InvalidArgument(format!(
                        "temperature must fall between {MIN_TEMPERATURE} and {DAY} kelvin"
                    ))));
                }
                Some(kelvin) => match self.boundary(Utc::now()) {
                    Some((phase, _)) => {
                        self.manual = Some((kelvin, phase));
                        pending = Some(reply);
                    }
                    None => {
                        let error = if self.effective() == Schedule::Off {
                            CommandError::InvalidArgument(
                                "there is nothing to override while the night light is off"
                                    .to_owned(),
                            )
                        } else {
                            CommandError::Unavailable(self.missing().to_owned())
                        };
                        let _ = reply.send(Err(error));
                    }
                },
                None => {
                    self.manual = None;
                    pending = Some(reply);
                }
            },
            Input::Config(config) => {
                self.forced = None;
                self.manual = None;
                self.config = config;
            }
        }
        self.evaluate(ctx, Utc::now()).await;
        if let Some(reply) = pending {
            let _ = reply.send(Ok(()));
        }
    }

    /// The outputs go back before the process does. A `SIGKILL` cannot run this, and the ramp then
    /// outlives the process — that is the protocol, and the README says so rather than guarding it.
    async fn stop(mut self, _ctx: &Ctx<Self>) {
        if let Err(reason) = self.hand_back() {
            tracing::warn!(
                reason,
                "the display was left at the night light's temperature"
            );
        }
    }
}

impl NightLight {
    fn cadence(&self, now: DateTime<Utc>) -> time::Duration {
        match self.boundary(now) {
            Some((_, Some(change))) if in_transition(change - now, self.config.transition) => {
                RAMPING
            }
            _ => TICK,
        }
    }

    fn effective(&self) -> Schedule {
        self.forced.unwrap_or(self.config.schedule)
    }

    async fn evaluate(&mut self, ctx: &Ctx<Self>, now: DateTime<Utc>) {
        self.settle(ctx, now);
        self.publish();
    }

    fn settle(&mut self, ctx: &Ctx<Self>, now: DateTime<Utc>) {
        if self.effective() == Schedule::Off {
            return self.release(ctx);
        }
        let Some((phase, next_change)) = self.boundary(now) else {
            return ctx.degraded(self.missing());
        };

        if let Some((_, manual_phase)) = &self.manual
            && *manual_phase != phase
        {
            self.manual = None;
        }

        // Applied on every tick rather than only when it moves: the compositor is the authority on
        // whether we still hold the outputs, and a client that took them from us and left would
        // otherwise leave the screen neutral while this service still believed it had applied.
        let wanted = match self.manual {
            Some((kelvin, _)) => kelvin,
            None => ramp(&phase, next_change, now, &self.config),
        };
        if wanted == DAY && (self.manual.is_some() || phase == SolarPhase::Day) {
            return self.release(ctx);
        }
        match self.gamma.apply(wanted) {
            Ok(()) => {
                self.applied = Some(wanted);
                ctx.running();
            }
            Err(reason) => ctx.degraded(reason),
        }
    }

    /// `Off` hands the outputs back rather than applying neutral daylight — the compositor's own
    /// ramp is not necessarily 6500K, and pinning it there is still holding it.
    fn release(&mut self, ctx: &Ctx<Self>) {
        match self.hand_back() {
            Ok(()) => ctx.running(),
            Err(reason) => ctx.degraded(reason),
        }
    }

    /// Releasing what was never taken is not a failure, and resetting twice is not a second one.
    fn hand_back(&mut self) -> Result<(), String> {
        if self.applied.is_none() {
            return Ok(());
        }
        self.gamma.reset()?;
        self.applied = None;
        Ok(())
    }

    fn boundary(&self, now: DateTime<Utc>) -> Option<(SolarPhase, Option<DateTime<Utc>>)> {
        match self.effective() {
            Schedule::Off => None,
            Schedule::Automatic => self
                .observed
                .clone()
                .map(|status| (status.phase, status.next_change)),
            Schedule::Schedule => Some(manual(self.config.start?, self.config.end?, now)),
        }
    }

    fn missing(&self) -> &'static str {
        match self.effective() {
            Schedule::Automatic => "the settings provider has no solar phase",
            _ => "a schedule needs start-time and end-time, each written as HH:MM",
        }
    }

    fn publish(&self) {
        self.state.set(NightLightState {
            schedule: self.effective(),
            overridden: self.forced.is_some(),
            temperature: self.applied.unwrap_or(DAY),
            target: self.config.temperature,
            configured: self.config.schedule,
            manual: self.manual.is_some(),
        });
    }
}

/// The temperature standing at `now`, ramping so that it arrives at the next phase's own
/// temperature exactly as the phase flips. Anchoring on the boundary ahead is what lets this read
/// one instant instead of remembering the last one.
fn ramp(
    phase: &SolarPhase,
    next_change: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    config: &Config,
) -> u32 {
    let (steady, next) = match phase {
        SolarPhase::Day => (DAY, config.temperature),
        SolarPhase::Night => (config.temperature, DAY),
    };
    let Some(change) = next_change else {
        return steady;
    };
    let remaining = change - now;
    // A boundary already behind us means the ramp finished: `solar` recomputes on its own minute,
    // so just after sunset the phase still reads `Day` against a `next_change` in the past. Reading
    // that as "far from the boundary" put daylight back on the screen for up to a minute, every
    // sunset and every sunrise.
    if remaining <= TimeDelta::zero() {
        return next;
    }
    if !in_transition(remaining, config.transition) {
        return steady;
    }

    let progress = 1.0 - remaining.as_seconds_f64() / config.transition.as_seconds_f64();
    let travelled = (next as f64 - steady as f64) * progress.clamp(0.0, 1.0);
    (steady as f64 + travelled).round() as u32
}

fn in_transition(remaining: TimeDelta, transition: TimeDelta) -> bool {
    remaining > TimeDelta::zero() && remaining < transition
}

/// Night runs from `start` to `end` every day in the machine's own zone. Both instants are laid out
/// across yesterday, today and tomorrow and then read in order, so an overnight window needs no
/// branch of its own and neither does the midnight it crosses.
fn manual(
    start: NaiveTime,
    end: NaiveTime,
    now: DateTime<Utc>,
) -> (SolarPhase, Option<DateTime<Utc>>) {
    if start == end {
        return (SolarPhase::Day, None);
    }

    let today = now.with_timezone(&Local).date_naive();
    let mut marks: Vec<(DateTime<Utc>, SolarPhase)> = (-1..=1)
        .filter_map(|offset| today.checked_add_signed(TimeDelta::days(offset)))
        .flat_map(|date| {
            [
                (date, start, SolarPhase::Night),
                (date, end, SolarPhase::Day),
            ]
        })
        .filter_map(|(date, time, phase)| {
            // `earliest`, not `single`: an hour repeated by a daylight-saving fall-back is
            // ambiguous rather than absent, and dropping it leaves a night that never starts.
            let local = date.and_time(time).and_local_timezone(Local).earliest()?;
            Some((local.with_timezone(&Utc), phase))
        })
        .collect();
    marks.sort_by_key(|(instant, _)| *instant);

    let phase = marks
        .iter()
        .rev()
        .find(|(instant, _)| *instant <= now)
        .map_or(SolarPhase::Day, |(_, phase)| phase.clone());
    let next = marks
        .iter()
        .find(|(instant, _)| *instant > now)
        .map(|(instant, _)| *instant);
    (phase, next)
}

#[cfg(test)]
mod tests;
