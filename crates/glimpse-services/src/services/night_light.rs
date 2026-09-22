use crate::services::{SolarPhase, SolarStatus};
use chrono::{DateTime, Local, NaiveTime, TimeDelta, Utc};
use glimpse_config::Schedule;
use tokio::{
    sync::{oneshot, watch},
    time,
};

use crate::{
    ServiceState,
    context::Ctx,
    gamma::Gamma,
    publisher::Publisher,
    service::{CommandError, Input, Service, ServiceEndpoint, ServiceError},
    services::solar::SolarHandle,
    subscription::Sub,
};

/// Neutral daylight. Nothing is applied at this temperature; it is the value both ramps return to,
/// and the one the gamma curve is normalized against.
pub const DAY: u32 = 6500;

const MIN_MANUAL_TEMPERATURE: u32 = 1000;

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
    pub solar: SolarHandle,
    pub gamma: Box<dyn Gamma>,
}

#[derive(Clone)]
pub struct NightLightHandle(ServiceEndpoint<NightLight>);

impl NightLightHandle {
    pub fn snapshot(&self) -> NightLightState {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> watch::Receiver<NightLightState> {
        self.0.subscribe()
    }

    pub fn health(&self) -> watch::Receiver<ServiceState> {
        self.0.health()
    }

    pub async fn set_schedule(&self, schedule: Schedule) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(Command::SetSchedule { schedule, reply })?;
        result.await.map_err(|_| {
            CommandError::Unavailable("night light stopped before accepting the mode".to_owned())
        })?
    }

    pub async fn set_temperature(&self, kelvin: Option<u32>) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(Command::SetTemperature { kelvin, reply })?;
        result.await.map_err(|_| {
            CommandError::Unavailable(
                "night light stopped before accepting the temperature".to_owned(),
            )
        })?
    }
}

pub struct NightLight {
    state: Publisher<NightLightState>,
    solar: SolarHandle,
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
    type Handle = NightLightHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = Dependencies;
    type SubKey = Watch;

    fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle {
        NightLightHandle(endpoint)
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
                self.solar.subscribe(),
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
    ) -> Result<Self, ServiceError> {
        Ok(Self {
            state: ctx.publisher(),
            solar: dependencies.solar,
            gamma: dependencies.gamma,
            config,
            forced: None,
            manual: None,
            observed: None,
            applied: None,
        })
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
                Some(kelvin) if !(MIN_MANUAL_TEMPERATURE..=DAY).contains(&kelvin) => {
                    let _ = reply.send(Err(CommandError::InvalidArgument(format!(
                        "temperature must fall between {MIN_MANUAL_TEMPERATURE} and {DAY} kelvin"
                    ))));
                }
                Some(kelvin) => match self.boundary(Utc::now()) {
                    Some((phase, _)) => {
                        self.manual = Some((kelvin, phase));
                        pending = Some(reply);
                    }
                    None => {
                        let reason = if self.effective() == Schedule::Off {
                            "there is nothing to override while the night light is off"
                        } else {
                            self.missing()
                        };
                        let _ = reply.send(Err(CommandError::Unavailable(reason.to_owned())));
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
                .or_else(|| self.solar.snapshot())
                .map(|status| (status.phase, status.next_change)),
            Schedule::Schedule => Some(manual(self.config.start?, self.config.end?, now)),
        }
    }

    fn missing(&self) -> &'static str {
        match self.effective() {
            Schedule::Automatic => "there is no location fix yet",
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
mod tests {
    use glimpse_dbus::Buses;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::{gamma::FakeGamma, service::ServiceRuntime, services::solar::Solar};

    const NIGHT: u32 = 4200;

    struct Harness {
        service: NightLight,
        gamma: FakeGamma,
        ctx: Ctx<NightLight>,
        health: watch::Receiver<ServiceState>,
        state: watch::Receiver<NightLightState>,
        _inbox: mpsc::Receiver<Input<NightLight>>,
        _cancel: CancellationToken,
    }

    impl Harness {
        async fn at(&mut self, now: DateTime<Utc>) {
            self.service.evaluate(&self.ctx, now).await;
        }

        async fn feed(&mut self, input: Input<NightLight>) {
            self.service.handle(&self.ctx, input).await;
        }

        async fn set(&mut self, schedule: Schedule) {
            let (reply, result) = oneshot::channel();
            self.feed(Input::Command(Command::SetSchedule { schedule, reply }))
                .await;
            result
                .await
                .expect("the handler answers")
                .expect("the mode is accepted");
        }

        async fn set_temperature(&mut self, kelvin: Option<u32>) -> Result<(), CommandError> {
            let (reply, result) = oneshot::channel();
            self.feed(Input::Command(Command::SetTemperature { kelvin, reply }))
                .await;
            result.await.expect("the handler answers")
        }

        fn reason(&self) -> Option<String> {
            match &*self.health.borrow() {
                ServiceState::Degraded { reason } => Some(reason.clone()),
                _ => None,
            }
        }
    }

    fn config(schedule: Schedule) -> Config {
        Config {
            schedule,
            temperature: NIGHT,
            start: None,
            end: None,
            transition: TimeDelta::minutes(15),
        }
    }

    async fn harness(config: Config) -> Harness {
        let (events, inbox) = mpsc::channel(32);
        let cancel = CancellationToken::new();
        let buses = Buses::unavailable("no bus in tests");
        let (solar_runtime, solar) =
            ServiceRuntime::<Solar>::new(crate::NoConfig, buses.clone(), cancel.clone());
        drop(solar_runtime);
        let (health, health_rx) = watch::channel(ServiceState::Starting);
        let (published, state) = watch::channel(initial_state(&config));
        let ctx = Ctx::<NightLight>::new(events, &cancel, published, health, buses);
        let gamma = FakeGamma::default();
        let service = NightLight::start(
            &ctx,
            config,
            Dependencies {
                solar,
                gamma: Box::new(gamma.clone()),
            },
        )
        .await
        .expect("the service starts");

        Harness {
            service,
            gamma,
            ctx,
            health: health_rx,
            state,
            _inbox: inbox,
            _cancel: cancel,
        }
    }

    fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        use chrono::TimeZone as _;
        Utc.with_ymd_and_hms(2026, 6, 21, hour, minute, 0)
            .single()
            .expect("one instant")
    }

    fn sunset() -> DateTime<Utc> {
        at(20, 0)
    }

    fn day(next_change: Option<DateTime<Utc>>) -> Option<SolarStatus> {
        Some(SolarStatus {
            phase: SolarPhase::Day,
            next_change,
        })
    }

    fn night(next_change: Option<DateTime<Utc>>) -> Option<SolarStatus> {
        Some(SolarStatus {
            phase: SolarPhase::Night,
            next_change,
        })
    }

    #[tokio::test]
    async fn a_schedule_that_is_off_applies_nothing() {
        let mut harness = harness(config(Schedule::Off)).await;

        harness.at(at(23, 0)).await;

        assert!(harness.gamma.applied().is_empty());
        assert_eq!(harness.gamma.resets(), 0, "nothing was held to release");
        assert!(!harness.state.borrow().active());
    }

    #[tokio::test]
    async fn far_from_the_boundary_each_phase_holds_its_own_temperature() {
        let mut harness = harness(config(Schedule::Automatic)).await;

        harness.service.observed = day(Some(sunset()));
        harness.at(at(12, 0)).await;
        assert!(
            harness.gamma.applied().is_empty(),
            "daylight is the absence of a ramp, not a curve that happens to be neutral"
        );
        assert!(!harness.state.borrow().active());

        // Tomorrow's sunrise, because `solar` never reports a boundary that has already passed.
        harness.service.observed = night(Some(at(5, 0) + TimeDelta::days(1)));
        harness.at(at(23, 0)).await;
        assert_eq!(harness.gamma.applied(), vec![NIGHT]);
        assert!(harness.state.borrow().active());
    }

    /// The property the whole `next_change` design exists for: the ramp arrives exactly as the
    /// phase flips, so the two never disagree about what the screen should look like.
    #[tokio::test]
    async fn the_ramp_reaches_the_target_exactly_at_the_boundary() {
        let mut harness = harness(config(Schedule::Automatic)).await;
        harness.service.observed = day(Some(sunset()));

        harness.at(sunset() - TimeDelta::minutes(15)).await;
        assert!(
            harness.gamma.applied().is_empty(),
            "the ramp opens at daylight, which is nothing applied"
        );

        harness.at(sunset() - TimeDelta::minutes(8)).await;
        harness.at(sunset()).await;

        let applied = harness.gamma.applied();
        assert_eq!(applied.last(), Some(&NIGHT), "and closes at the target");
        let midpoint = applied[0];
        assert!(
            midpoint < DAY && midpoint > NIGHT,
            "{midpoint}K is between the two"
        );
    }

    /// `solar` recomputes on its own minute, so between a boundary passing and the next solar tick
    /// the phase is stale against a `next_change` in the past. Treating that as "far away" is what
    /// flashed daylight onto the screen at every sunset.
    #[tokio::test]
    async fn a_boundary_already_behind_us_holds_the_temperature_it_ramped_to() {
        let mut harness = harness(config(Schedule::Automatic)).await;
        harness.service.observed = day(Some(sunset()));

        harness.at(sunset() + TimeDelta::seconds(30)).await;

        assert_eq!(harness.gamma.applied(), vec![NIGHT]);
    }

    #[tokio::test]
    async fn a_zero_transition_steps_rather_than_ramps() {
        let mut harness = harness(Config {
            transition: TimeDelta::zero(),
            ..config(Schedule::Automatic)
        })
        .await;
        harness.service.observed = day(Some(sunset()));

        harness.at(sunset() - TimeDelta::minutes(1)).await;
        assert!(
            harness.gamma.applied().is_empty(),
            "a minute before the boundary a zero transition has begun no ramp"
        );

        harness.service.observed = night(Some(at(5, 0) + TimeDelta::days(1)));
        harness.at(sunset()).await;

        assert_eq!(
            harness.gamma.applied(),
            vec![NIGHT],
            "it steps straight to the target instead of travelling to it"
        );
    }

    /// A summer night above fifty degrees is shorter than a generous transition, so the two ramps
    /// overlap. Reading only the boundary ahead still answers at every instant — the screen simply
    /// never reaches the full temperature, which is the honest result rather than a glitch.
    #[tokio::test]
    async fn a_night_shorter_than_the_transition_never_reaches_full_temperature() {
        let mut harness = harness(Config {
            transition: TimeDelta::hours(2),
            ..config(Schedule::Automatic)
        })
        .await;
        harness.service.observed = night(Some(at(1, 0)));

        harness.at(at(0, 30)).await;

        let applied = *harness
            .gamma
            .applied()
            .last()
            .expect("something was applied");
        assert!(
            applied > NIGHT && applied < DAY,
            "{applied}K is mid-ramp rather than either end"
        );
    }

    #[tokio::test]
    async fn a_polar_day_holds_steady_with_no_boundary_to_ramp_to() {
        let mut harness = harness(config(Schedule::Automatic)).await;
        harness.service.observed = night(None);

        harness.at(at(2, 0)).await;

        assert_eq!(harness.gamma.applied(), vec![NIGHT]);
    }

    #[tokio::test]
    async fn automatic_without_a_fix_degrades_and_applies_nothing() {
        let mut harness = harness(config(Schedule::Automatic)).await;

        harness.at(at(12, 0)).await;

        assert!(harness.gamma.applied().is_empty());
        assert_eq!(
            harness.reason().as_deref(),
            Some("there is no location fix yet")
        );
    }

    #[tokio::test]
    async fn a_schedule_without_times_degrades_and_names_both_keys() {
        let mut harness = harness(config(Schedule::Schedule)).await;

        harness.at(at(12, 0)).await;

        assert!(harness.gamma.applied().is_empty());
        assert!(
            harness
                .reason()
                .is_some_and(|reason| reason.contains("HH:MM"))
        );
    }

    #[tokio::test]
    async fn a_failing_backend_degrades_and_a_later_success_clears_it() {
        let mut harness = harness(config(Schedule::Automatic)).await;
        harness.service.observed = night(None);
        harness
            .gamma
            .fail(Some("another gamma client holds the outputs"));

        harness.at(at(23, 0)).await;
        assert!(
            harness
                .reason()
                .is_some_and(|reason| reason.contains("another gamma client"))
        );

        harness.gamma.fail(None);
        harness.at(at(23, 1)).await;

        assert_eq!(harness.gamma.applied(), vec![NIGHT]);
        assert!(matches!(&*harness.health.borrow(), ServiceState::Running));
    }

    /// Measured against a live compositor: a second gamma client takes the outputs and, on leaving,
    /// hands them back to nobody. Re-applying every tick is what brings them back; suppressing the
    /// call while the temperature had not moved left the screen neutral for as long as it did not.
    #[tokio::test]
    async fn the_temperature_is_reapplied_every_tick_so_stolen_outputs_come_back() {
        let mut harness = harness(config(Schedule::Automatic)).await;
        harness.service.observed = night(None);

        harness.at(at(23, 0)).await;
        harness.at(at(23, 1)).await;
        harness.at(at(23, 2)).await;

        assert_eq!(harness.gamma.applied(), vec![NIGHT, NIGHT, NIGHT]);
        assert_eq!(harness.state.borrow().temperature, NIGHT);
    }

    #[tokio::test]
    async fn turning_the_schedule_off_releases_the_outputs_once() {
        let mut harness = harness(config(Schedule::Automatic)).await;
        harness.service.observed = night(None);
        harness.at(at(23, 0)).await;
        assert_eq!(harness.gamma.applied(), vec![NIGHT]);

        harness.service.config.schedule = Schedule::Off;
        harness.at(at(23, 1)).await;
        harness.at(at(23, 2)).await;

        assert_eq!(harness.gamma.resets(), 1, "released once, not per tick");
        assert!(!harness.state.borrow().active());
    }

    #[tokio::test]
    async fn a_day_temperature_is_released_rather_than_written_as_a_neutral_curve() {
        let mut harness = harness(config(Schedule::Automatic)).await;
        harness.service.observed = night(None);
        harness.at(at(23, 0)).await;
        assert_eq!(harness.gamma.applied(), vec![NIGHT]);

        harness.service.observed = day(Some(sunset()));
        harness.at(at(12, 0)).await;

        assert_eq!(
            harness.gamma.applied(),
            vec![NIGHT],
            "nothing further was written; the outputs went back instead"
        );
        assert_eq!(harness.gamma.resets(), 1);
        assert_eq!(harness.state.borrow().temperature, DAY);
    }

    #[tokio::test]
    async fn a_night_ramp_that_rounds_to_daylight_keeps_gamma_control_until_sunrise() {
        let mut harness = harness(Config {
            temperature: DAY - 1,
            ..config(Schedule::Automatic)
        })
        .await;
        harness.service.observed = night(Some(at(5, 0)));

        harness.at(at(4, 59)).await;

        assert_eq!(harness.gamma.applied(), vec![DAY]);
        assert_eq!(harness.gamma.resets(), 0);
    }

    #[tokio::test]
    async fn a_commanded_mode_wins_over_the_document_and_says_that_it_did() {
        let mut harness = harness(config(Schedule::Automatic)).await;
        harness.service.observed = night(None);
        harness.at(at(23, 0)).await;
        assert!(harness.state.borrow().active());

        harness.set(Schedule::Off).await;

        assert_eq!(harness.gamma.resets(), 1, "the outputs go back");
        assert!(!harness.state.borrow().active());
        assert_eq!(harness.state.borrow().schedule, Schedule::Off);
        assert!(harness.state.borrow().overridden);
    }

    #[tokio::test]
    async fn editing_the_night_light_table_takes_the_mode_back() {
        let mut harness = harness(config(Schedule::Automatic)).await;
        harness.service.observed = night(None);
        harness.set(Schedule::Off).await;
        assert!(harness.state.borrow().overridden);

        let mut edited = config(Schedule::Automatic);
        edited.temperature = 3000;
        harness.feed(Input::Config(edited)).await;

        assert!(!harness.state.borrow().overridden);
        assert_eq!(harness.state.borrow().schedule, Schedule::Automatic);
    }

    #[tokio::test]
    async fn the_ramp_is_sampled_faster_than_it_moves_while_it_is_moving() {
        let mut harness = harness(config(Schedule::Automatic)).await;
        harness.service.observed = day(Some(sunset()));

        assert_eq!(
            harness.service.cadence(sunset() - TimeDelta::hours(2)),
            TICK,
            "far from the boundary there is nothing to draw"
        );
        assert_eq!(
            harness.service.cadence(sunset() - TimeDelta::minutes(5)),
            RAMPING,
            "inside the transition the screen is actually moving"
        );
        assert_eq!(
            harness.service.cadence(sunset() + TimeDelta::minutes(1)),
            TICK,
            "the ramp finished at the boundary"
        );
    }

    #[tokio::test]
    async fn a_ramp_step_is_smaller_than_the_eye_resolves() {
        let config = config(Schedule::Automatic);
        let span = (DAY as f64 - config.temperature as f64).abs();
        let steps = config.transition.as_seconds_f64() / RAMPING.as_secs_f64();

        assert!(
            span / steps < 50.0,
            "a {}K step is coarse enough to read as a staircase",
            span / steps
        );
    }

    #[tokio::test]
    async fn a_schedule_with_no_transition_never_asks_to_be_woken_faster() {
        let mut settings = config(Schedule::Automatic);
        settings.transition = TimeDelta::zero();
        let mut harness = harness(settings).await;
        harness.service.observed = day(Some(sunset()));

        assert_eq!(
            harness.service.cadence(sunset() - TimeDelta::seconds(1)),
            TICK
        );
    }

    #[test]
    fn a_document_edited_outside_this_table_projects_to_the_same_configuration() {
        let mut document = glimpse_config::Config::default();
        let before = Config::from(&document);

        document.weather.forecast_days += 1;

        assert_eq!(
            Config::from(&document),
            before,
            "the runtime can only spare this service a reload while its own slice is unchanged, \
             so the mode it was commanded into survives an edit to someone else's table"
        );
    }

    #[tokio::test]
    async fn an_override_declares_the_sources_its_mode_needs() {
        let mut harness = harness(config(Schedule::Automatic)).await;
        assert_eq!(harness.service.subscriptions().len(), 2);

        harness.set(Schedule::Off).await;
        assert!(harness.service.subscriptions().is_empty());

        harness.set(Schedule::Schedule).await;
        assert_eq!(
            harness.service.subscriptions().len(),
            1,
            "a manual window needs the tick but not the solar watch"
        );
    }

    #[tokio::test]
    async fn a_release_the_backend_refused_is_retried_rather_than_forgotten() {
        let mut harness = harness(config(Schedule::Automatic)).await;
        harness.service.observed = night(None);
        harness.at(at(23, 0)).await;
        assert_eq!(harness.gamma.applied(), vec![NIGHT]);

        harness
            .gamma
            .fail(Some("another gamma client holds the outputs"));
        harness.set(Schedule::Off).await;
        assert_eq!(harness.gamma.resets(), 0, "the backend refused");
        assert!(harness.reason().is_some(), "and the service says so");

        harness.gamma.fail(None);
        harness.set(Schedule::Off).await;

        assert_eq!(harness.gamma.resets(), 1, "the release is still owed");
        assert!(harness.reason().is_none());
    }

    #[tokio::test]
    async fn a_degraded_service_still_publishes_the_mode_in_force() {
        let mut harness = harness(config(Schedule::Schedule)).await;
        harness.set(Schedule::Off).await;
        assert_eq!(harness.state.borrow().schedule, Schedule::Off);

        harness.set(Schedule::Automatic).await;

        assert_eq!(harness.state.borrow().schedule, Schedule::Automatic);
        assert!(harness.state.borrow().overridden);
        assert_eq!(
            harness.reason().as_deref(),
            Some("there is no location fix yet")
        );
    }

    #[tokio::test]
    async fn a_refused_ramp_publishes_the_mode_and_the_temperature_still_showing() {
        let mut harness = harness(config(Schedule::Automatic)).await;
        harness.service.observed = night(None);
        harness.at(at(23, 0)).await;
        assert_eq!(harness.state.borrow().temperature, NIGHT);

        harness.gamma.fail(Some("no gamma control"));
        harness.at(at(23, 1)).await;

        assert_eq!(harness.state.borrow().schedule, Schedule::Automatic);
        assert_eq!(harness.state.borrow().temperature, NIGHT);
        assert_eq!(harness.reason().as_deref(), Some("no gamma control"));
    }

    #[tokio::test]
    async fn a_manual_temperature_is_applied_on_every_tick() {
        let mut harness = harness(config(Schedule::Automatic)).await;
        harness.service.observed = day(Some(sunset()));

        harness
            .set_temperature(Some(3000))
            .await
            .expect("3000 is in range");
        harness.at(at(12, 0)).await;
        harness.at(at(12, 1)).await;

        assert_eq!(harness.gamma.applied(), vec![3000, 3000, 3000]);
        assert!(harness.state.borrow().manual);
    }

    #[tokio::test]
    async fn a_boundary_crossing_clears_the_manual_override() {
        let mut harness = harness(config(Schedule::Automatic)).await;
        harness.service.observed = day(Some(sunset()));
        harness
            .set_temperature(Some(3000))
            .await
            .expect("3000 is in range");
        harness.at(at(12, 0)).await;
        assert!(harness.state.borrow().manual);

        harness.service.observed = night(None);
        harness.at(at(23, 0)).await;

        assert!(!harness.state.borrow().manual);
        assert_eq!(harness.gamma.applied().last(), Some(&NIGHT));
    }

    #[tokio::test]
    async fn setting_the_schedule_clears_a_manual_override() {
        let mut harness = harness(config(Schedule::Automatic)).await;
        harness.service.observed = day(Some(sunset()));
        harness
            .set_temperature(Some(3000))
            .await
            .expect("3000 is in range");
        assert!(harness.state.borrow().manual);

        harness.set(Schedule::Automatic).await;

        assert!(!harness.state.borrow().manual);
    }

    #[tokio::test]
    async fn clearing_the_override_lets_the_ramp_resume_on_the_next_evaluate() {
        let mut harness = harness(config(Schedule::Automatic)).await;
        harness.service.observed = day(Some(sunset()));
        harness
            .set_temperature(Some(3000))
            .await
            .expect("3000 is in range");
        assert!(harness.state.borrow().manual);

        harness
            .set_temperature(None)
            .await
            .expect("clearing is always accepted");
        harness.at(at(12, 0)).await;

        assert!(!harness.state.borrow().manual);
        assert_eq!(
            harness.gamma.resets(),
            1,
            "the ramp resumed at daylight, which releases rather than applies"
        );
        assert_eq!(harness.state.borrow().temperature, DAY);
        assert!(!harness.state.borrow().active());
    }

    #[tokio::test]
    async fn an_out_of_range_temperature_is_refused_without_quoting_what_arrived() {
        let mut harness = harness(config(Schedule::Automatic)).await;
        harness.service.observed = night(None);

        let low = harness
            .set_temperature(Some(999))
            .await
            .expect_err("too cold");
        let high = harness
            .set_temperature(Some(6501))
            .await
            .expect_err("too warm");

        for (rejected, value) in [(&low, "999"), (&high, "6501")] {
            assert!(matches!(rejected, CommandError::InvalidArgument(_)));
            assert!(
                !rejected.to_string().contains(value),
                "the rejected value must not be echoed, got {rejected}"
            );
        }
        assert!(!harness.state.borrow().manual);
        assert!(!harness.gamma.applied().contains(&999));
        assert!(!harness.gamma.applied().contains(&6501));
    }

    #[test]
    fn a_fresh_service_has_no_manual_override_and_reports_the_configured_schedule() {
        let state = initial_state(&config(Schedule::Schedule));

        assert!(!state.manual);
        assert_eq!(state.configured, Schedule::Schedule);
    }

    #[tokio::test]
    async fn an_override_does_not_resurrect_a_released_ramp() {
        let mut harness = harness(config(Schedule::Automatic)).await;
        harness.service.observed = night(None);
        harness.at(at(23, 0)).await;
        assert_eq!(harness.gamma.applied(), vec![NIGHT]);

        harness.set(Schedule::Off).await;
        assert_eq!(harness.gamma.resets(), 1);
        assert!(!harness.state.borrow().active());

        let refused = harness.set_temperature(Some(3000)).await;
        assert!(refused.is_err(), "there is nothing to override while off");

        harness.at(at(23, 1)).await;

        assert_eq!(
            harness.gamma.resets(),
            1,
            "hand_back is a no-op, not a second reset"
        );
        assert_eq!(harness.gamma.applied(), vec![NIGHT]);
    }

    #[tokio::test]
    async fn a_temperature_is_refused_while_the_night_light_is_off() {
        let mut harness = harness(config(Schedule::Off)).await;

        let refused = harness
            .set_temperature(Some(3000))
            .await
            .expect_err("off has nothing to override");

        assert!(matches!(refused, CommandError::Unavailable(_)));
        assert!(harness.gamma.applied().is_empty());
    }

    #[tokio::test]
    async fn a_temperature_is_refused_under_automatic_with_no_fix() {
        let mut harness = harness(config(Schedule::Automatic)).await;

        let refused = harness
            .set_temperature(Some(3000))
            .await
            .expect_err("there is no phase to anchor the override to yet");

        assert!(matches!(refused, CommandError::Unavailable(_)));
        assert!(!harness.state.borrow().manual);
        assert!(harness.gamma.applied().is_empty());
    }

    /// The window crosses midnight, which is the case a same-day comparison gets backwards.
    #[test]
    fn an_overnight_window_is_night_on_both_sides_of_midnight() {
        let start = NaiveTime::from_hms_opt(22, 0, 0).expect("a real time");
        let end = NaiveTime::from_hms_opt(6, 0, 0).expect("a real time");
        let local = |hour| {
            use chrono::TimeZone as _;
            Local
                .with_ymd_and_hms(2026, 6, 21, hour, 30, 0)
                .single()
                .expect("one instant")
                .with_timezone(&Utc)
        };

        assert_eq!(manual(start, end, local(23)).0, SolarPhase::Night);
        assert_eq!(manual(start, end, local(2)).0, SolarPhase::Night);
        assert_eq!(manual(start, end, local(12)).0, SolarPhase::Day);
    }

    #[test]
    fn a_manual_boundary_is_always_still_ahead() {
        let start = NaiveTime::from_hms_opt(22, 0, 0).expect("a real time");
        let end = NaiveTime::from_hms_opt(6, 0, 0).expect("a real time");
        let now = at(12, 0);

        let (_, next) = manual(start, end, now);

        assert!(next.is_some_and(|next| next > now));
    }

    /// A zero-length night is a document saying "never", and reading it as "always" would tint the
    /// screen around the clock.
    #[test]
    fn a_window_with_matching_ends_is_never_night() {
        let noon = NaiveTime::from_hms_opt(12, 0, 0).expect("a real time");

        assert_eq!(manual(noon, noon, at(12, 0)), (SolarPhase::Day, None));
    }
}
