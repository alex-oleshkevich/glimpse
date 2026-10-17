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
const SOFTENING_STEP: time::Duration = time::Duration::from_secs(1);

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
    pub paused_until: Option<DateTime<Utc>>,
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
        paused_until: None,
    }
}

struct Ramp {
    from: u32,
    to: u32,
    started: DateTime<Utc>,
    duration: TimeDelta,
}

impl Ramp {
    const GLIDE: TimeDelta = TimeDelta::seconds(3);

    fn to_boundary(phase: &SolarPhase, change: DateTime<Utc>, config: &Config) -> Self {
        Self {
            from: steady(phase, config),
            to: match phase {
                SolarPhase::Day => config.temperature,
                SolarPhase::Night => DAY,
            },
            started: change - config.transition,
            duration: config.transition,
        }
    }

    fn glide(from: u32, to: u32, now: DateTime<Utc>) -> Self {
        Self {
            from,
            to,
            started: now,
            duration: Self::GLIDE,
        }
    }

    fn value(&self, now: DateTime<Utc>) -> u32 {
        if self.duration <= TimeDelta::zero() {
            return if now >= self.started {
                self.to
            } else {
                self.from
            };
        }
        let progress = ((now - self.started).as_seconds_f64() / self.duration.as_seconds_f64())
            .clamp(0.0, 1.0);
        (self.from as f64 + (self.to as f64 - self.from as f64) * progress).round() as u32
    }

    fn moving(&self, now: DateTime<Utc>) -> bool {
        self.duration > TimeDelta::zero()
            && now >= self.started
            && now < self.started + self.duration
    }
}

fn steady(phase: &SolarPhase, config: &Config) -> u32 {
    match phase {
        SolarPhase::Day => DAY,
        SolarPhase::Night => config.temperature,
    }
}

pub enum Event {
    Solar(Option<SolarStatus>),
    Tick,
}

pub enum Command {
    Pause {
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    Resume {
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
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
    pub async fn pause(&self) -> Result<(), CommandError> {
        self.call("night light stopped before pausing", |reply| {
            Command::Pause { reply }
        })
        .await?
    }

    pub async fn resume(&self) -> Result<(), CommandError> {
        self.call("night light stopped before resuming", |reply| {
            Command::Resume { reply }
        })
        .await?
    }

    pub async fn set_schedule(&self, schedule: Schedule) -> Result<(), CommandError> {
        self.call("night light stopped before accepting the mode", |reply| {
            Command::SetSchedule { schedule, reply }
        })
        .await?
    }

    pub async fn set_temperature(&self, kelvin: Option<u32>) -> Result<(), CommandError> {
        self.call(
            "night light stopped before accepting the temperature",
            |reply| Command::SetTemperature { kelvin, reply },
        )
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
    glide: Option<Ramp>,
    paused_until: Option<DateTime<Utc>>,
}

impl Service for NightLight {
    const NAME: &'static str = "night-light";

    type Config = Config;
    type State = NightLightState;
    type Command = Command;
    type Event = Event;
    type Dependencies = Dependencies;
    type SubKey = Watch;

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
            glide: None,
            paused_until: None,
        }
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        self.handle_at(ctx, input, Utc::now()).await;
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
    async fn handle_at(&mut self, ctx: &Ctx<Self>, input: Input<Self>, now: DateTime<Utc>) {
        let mut pending = None;
        match input {
            Input::Event(Event::Solar(observed)) => self.observed = observed,
            Input::Event(Event::Tick) => {}
            Input::Command(Command::Pause { reply }) => match self.boundary(now) {
                Some((SolarPhase::Night, Some(deadline))) if deadline > now => {
                    self.paused_until = Some(deadline);
                    pending = Some(reply);
                }
                Some((SolarPhase::Day, _)) => {
                    let _ = reply.send(Err(CommandError::Unsupported(
                        "there is no active night to pause".to_owned(),
                    )));
                }
                None if self.effective() == Schedule::Off => {
                    let _ = reply.send(Err(CommandError::Unsupported(
                        "there is no active night to pause".to_owned(),
                    )));
                }
                _ => {
                    let _ = reply.send(Err(CommandError::Unavailable(
                        "the next night boundary is unavailable".to_owned(),
                    )));
                }
            },
            Input::Command(Command::Resume { reply }) => {
                self.paused_until = None;
                pending = Some(reply);
            }
            Input::Command(Command::SetSchedule { schedule, reply }) => {
                self.forced = Some(schedule);
                self.paused_until = None;
                self.manual = None;
                pending = Some(reply);
            }
            Input::Command(Command::SetTemperature { kelvin, reply }) => match kelvin {
                Some(kelvin) if !(MIN_TEMPERATURE..=DAY).contains(&kelvin) => {
                    let _ = reply.send(Err(CommandError::InvalidArgument(format!(
                        "temperature must fall between {MIN_TEMPERATURE} and {DAY} kelvin"
                    ))));
                }
                Some(kelvin) => match self.boundary(now) {
                    Some((phase, _)) => {
                        self.manual = Some((kelvin, phase));
                        self.paused_until = None;
                        pending = Some(reply);
                    }
                    None => {
                        let error = if self.effective() == Schedule::Off {
                            CommandError::Unsupported(
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
                self.paused_until = None;
            }
        }
        self.evaluate(ctx, now).await;
        if let Some(reply) = pending {
            let _ = reply.send(Ok(()));
        }
    }

    fn cadence(&self, now: DateTime<Utc>) -> time::Duration {
        if self.glide.is_some() {
            return SOFTENING_STEP;
        }
        match self.boundary(now) {
            Some((phase, Some(change)))
                if Ramp::to_boundary(&phase, change, &self.config).moving(now) =>
            {
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
        if self.paused_until.is_some_and(|deadline| deadline <= now) {
            self.paused_until = None;
            self.manual = None;
        }
        if self.effective() == Schedule::Off {
            self.glide = None;
            return self.release(ctx);
        }
        if self.paused_until.is_some() {
            let wanted = self.glide_toward(now, DAY, false);
            return self.drive(ctx, wanted);
        }
        let Some((phase, next_change)) = self.boundary(now) else {
            self.glide = None;
            return ctx.degraded(self.missing());
        };

        if let Some((_, manual_phase)) = &self.manual
            && *manual_phase != phase
        {
            self.manual = None;
        }

        let boundary = next_change.map(|change| Ramp::to_boundary(&phase, change, &self.config));
        let (target, on_boundary_ramp) = match self.manual {
            Some((kelvin, _)) => (kelvin, false),
            None => match &boundary {
                Some(ramp) => (ramp.value(now), ramp.moving(now)),
                None => (steady(&phase, &self.config), false),
            },
        };
        let wanted = self.glide_toward(now, target, on_boundary_ramp);
        if wanted == target && target == DAY && (self.manual.is_some() || phase == SolarPhase::Day)
        {
            return self.release(ctx);
        }
        self.drive(ctx, wanted);
    }

    fn glide_toward(&mut self, now: DateTime<Utc>, target: u32, on_boundary_ramp: bool) -> u32 {
        if on_boundary_ramp {
            self.glide = None;
            return target;
        }
        let running = match self.glide.take() {
            Some(ramp) if ramp.to == target => ramp,
            _ => Ramp::glide(self.applied.unwrap_or(DAY), target, now),
        };
        let wanted = running.value(now);
        if wanted == target {
            return target;
        }
        self.glide = Some(running);
        wanted
    }

    /// Applied on every tick rather than only when it moves: the compositor is the authority on
    /// whether we still hold the outputs, and a client that took them from us and left would
    /// otherwise leave the screen neutral while this service still believed it had applied.
    fn drive(&mut self, ctx: &Ctx<Self>, wanted: u32) {
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
            paused_until: self.paused_until,
        });
    }
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
