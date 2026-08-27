use glimpse_contracts::{GeoCoordinates, GeolocationStatus, Message, SolarPhase, SolarStatus};
use sunrise::{Coordinates as SunriseCoordinates, SolarDay, SolarEvent};
use tokio::time;

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{Input, NoConfig, Service, ServiceError},
    subscription::Sub,
};

const TICK: time::Duration = time::Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Command {
    Refresh,
}

pub enum Event {
    Tick,
    Update(Option<GeoCoordinates>),
}

pub struct Solar {
    status: Publisher<SolarStatus>,
    coordinates: Option<GeoCoordinates>,
}

#[derive(PartialEq, Eq, Hash)]
pub enum Watch {
    Tick,
    Location,
}

impl Service for Solar {
    const NAME: &'static str = "solar";
    const TOPICS: &'static [&'static str] = &[SolarStatus::NAME];

    type Config = NoConfig;
    type Command = Command;
    type Event = Event;
    type SubKey = Watch;

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        vec![
            Sub::interval(Watch::Tick, TICK, |_ctx| async { Event::Tick }),
            Sub::topic::<GeolocationStatus>(Watch::Location, |data| {
                Event::Update(data.coordinates)
            }),
        ]
    }

    async fn start(ctx: &Ctx<Self>, _config: Self::Config) -> Result<Self, ServiceError> {
        tracing::debug!("starting solar service");
        Ok(Self {
            coordinates: None,
            status: ctx.publisher::<SolarStatus>(),
        })
    }

    async fn handle(&mut self, _ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Update(coordinates)) => {
                self.coordinates = coordinates;
            }
            Input::Event(Event::Tick) => self.refresh(),
            Input::Command(Command::Refresh, responder) => {
                self.refresh();
                responder.ok(());
            }
            Input::Config(NoConfig) => {}
        }
    }
}

impl Solar {
    fn refresh(&mut self) {
        let Some(coordinates) = self.coordinates.as_ref() else {
            tracing::warn!("solar: location unavailable");
            return;
        };
        if let Some(times) = solar_times_for_date(chrono::Local::now().date_naive(), coordinates) {
            self.status.set(SolarStatus {
                phase: detect_phase(&times),
            });
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SolarTimes {
    pub sunrise: Option<chrono::DateTime<chrono::Utc>>,
    pub sunset: Option<chrono::DateTime<chrono::Utc>>,
}

fn detect_phase(times: &SolarTimes) -> SolarPhase {
    SolarPhase::Day
}

fn solar_times_for_date(
    date: chrono::NaiveDate,
    coordinates: &GeoCoordinates,
) -> Option<SolarTimes> {
    let latlon = SunriseCoordinates::new(coordinates.latitude, coordinates.longitude)?;
    let solar_day = SolarDay::new(latlon, date);
    let sunrise = solar_day.event_time(SolarEvent::Sunrise);
    let sunset = solar_day.event_time(SolarEvent::Sunset);

    Some(SolarTimes {
        sunrise: sunrise,
        sunset: sunset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_topics_and_methods_exist() {
        crate::service::assert_declarations::<Solar>();
    }
}
