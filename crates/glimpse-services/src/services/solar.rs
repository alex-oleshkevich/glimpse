use std::f64::consts::TAU;

use crate::services::GeolocationStatus;
use chrono::{DateTime, Datelike, NaiveDate, TimeDelta, Utc};
use futures_util::{StreamExt as _, future, stream, stream::BoxStream};
use glimpse_dbus::login1::Login1ManagerProxy;
use glimpse_dbus::weather::GeoCoordinates;
use tokio::{
    sync::{oneshot, watch},
    time,
};

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, NoConfig, Service, ServiceEndpoint},
    subscription::Sub,
};
pub use glimpse_dbus::settings::{SolarPhase, SolarStatus};

const TICK: time::Duration = time::Duration::from_secs(60);

pub enum Command {
    Refresh {
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
}

pub enum Event {
    Tick,
    Update(Option<GeoCoordinates>),
}

pub struct Solar {
    status: Publisher<Option<SolarStatus>>,
    coordinates: Option<GeoCoordinates>,
    next_change: Option<DateTime<Utc>>,
    location: watch::Receiver<GeolocationStatus>,
}

pub struct SolarDependencies {
    pub geolocation: super::geolocation::GeolocationHandle,
}

pub type SolarHandle = ServiceEndpoint<Solar>;

impl ServiceEndpoint<Solar> {
    pub async fn refresh(&self) -> Result<(), CommandError> {
        self.call(|reply| Command::Refresh { reply }).await?
    }
}

#[derive(PartialEq, Eq, Hash)]
pub enum Watch {
    Tick,
    Change(DateTime<Utc>),
    Resume,
    Location,
}

impl Service for Solar {
    const NAME: &'static str = "solar";

    type Config = NoConfig;
    type State = Option<SolarStatus>;
    type Command = Command;
    type Event = Event;
    type Dependencies = SolarDependencies;
    type SubKey = Watch;

    fn command_stopped_reason(_command: &Self::Command) -> &'static str {
        "`solar` stopped before refreshing"
    }

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    /// The phase is re-evaluated at its next change, on resume, and every tick, since a clock set
    /// back after boot moves no deadline — and only once a location can produce one.
    fn subscriptions(&self) -> Vec<Sub<Self>> {
        let mut declared = vec![Sub::watch(
            Watch::Location,
            self.location.clone(),
            |data| Event::Update(data.coordinates),
            Event::Update(None),
        )];
        if self.coordinates.is_some() {
            declared.push(Sub::interval(Watch::Tick, TICK, |_ctx| async {
                Event::Tick
            }));
            if let Some(at) = self.next_change {
                declared.push(Sub::deadline(Watch::Change(at), at, Event::Tick));
            }
            declared.push(Sub::stream(Watch::Resume, resumes));
        }
        declared
    }

    async fn start(
        ctx: &Ctx<Self>,
        _config: Self::Config,
        dependencies: Self::Dependencies,
    ) -> Self {
        ctx.degraded("no location yet");
        Self {
            coordinates: None,
            next_change: None,
            status: ctx.publisher(),
            location: dependencies.geolocation.subscribe(),
        }
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Update(coordinates)) => {
                match &coordinates {
                    Some(_) => ctx.running(),
                    None => ctx.degraded("no location; the solar phase is unknown"),
                }
                self.coordinates = coordinates;
                if self.coordinates.is_none() {
                    self.next_change = None;
                    self.status.set(None);
                }
                self.refresh();
            }
            Input::Event(Event::Tick) => self.refresh(),
            Input::Command(Command::Refresh { reply }) => {
                self.refresh();
                let _ = reply.send(Ok(()));
            }
            Input::Config(NoConfig) => {}
        }
    }
}

impl Solar {
    fn refresh(&mut self) {
        let Some(coordinates) = self.coordinates.as_ref() else {
            return;
        };
        let now = Utc::now();
        if let Some(status) = status_at(now, solar_date(now, coordinates.longitude), coordinates) {
            self.next_change = status.next_change.filter(|at| *at > now);
            self.status.set(Some(status));
        }
    }
}

fn solar_date(now: DateTime<Utc>, longitude: f64) -> NaiveDate {
    (now + TimeDelta::seconds((longitude * 240.0) as i64)).date_naive()
}

async fn resumes(ctx: Ctx<Solar>) -> BoxStream<'static, Event> {
    let Ok(bus) = ctx.system_bus() else {
        return stream::empty().boxed();
    };
    let Ok(manager) = Login1ManagerProxy::new(bus).await else {
        return stream::empty().boxed();
    };
    match manager.receive_prepare_for_sleep().await {
        Ok(signals) => signals
            .filter_map(|signal| {
                future::ready(
                    signal
                        .args()
                        .is_ok_and(|args| !args.start)
                        .then_some(Event::Tick),
                )
            })
            .boxed(),
        Err(_) => stream::empty().boxed(),
    }
}

/// `None` only for coordinates out of range, which `geolocation` already refuses.
fn status_at(
    now: DateTime<Utc>,
    date: NaiveDate,
    coordinates: &GeoCoordinates,
) -> Option<SolarStatus> {
    // Both events are offsets from the same solar noon, so sunrise precedes sunset by construction.
    let status = match crate::sun::events(coordinates, date)? {
        (Some(sunrise), Some(sunset)) => match (now < sunrise, now < sunset) {
            (true, _) => SolarStatus {
                phase: SolarPhase::Night,
                next_change: Some(sunrise),
            },
            (false, true) => SolarStatus {
                phase: SolarPhase::Day,
                next_change: Some(sunset),
            },
            (false, false) => SolarStatus {
                phase: SolarPhase::Night,
                next_change: next_sunrise(coordinates, date),
            },
        },
        _ => SolarStatus {
            phase: polar_phase(date, coordinates.latitude),
            next_change: None,
        },
    };
    Some(status)
}

/// Tomorrow's sunrise, and `None` where tomorrow has none — so a consumer reads "the phase does not
/// change" rather than a time that never arrives.
fn next_sunrise(coordinates: &GeoCoordinates, date: NaiveDate) -> Option<DateTime<Utc>> {
    crate::sun::events(coordinates, date.succ_opt()?)?.0
}

/// Above the polar circles a date has neither event, and which way it goes follows from whether
/// that hemisphere is in its own summer. Only the sign of the solar declination is asked for, so
/// the axial tilt that Cooper's equation scales it by drops out and this is one cosine. The
/// approximation costs accuracy within about a day of an equinox, a window that reaches nowhere but
/// the poles themselves.
fn polar_phase(date: NaiveDate, latitude: f64) -> SolarPhase {
    let declination = -(TAU * (f64::from(date.ordinal()) + 10.0) / 365.0).cos();
    match declination.signum() == latitude.signum() {
        true => SolarPhase::Day,
        false => SolarPhase::Night,
    }
}

#[cfg(test)]
mod tests;
