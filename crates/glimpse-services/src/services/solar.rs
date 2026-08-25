use glimpse_contracts::{GeoCoordinates, GeolocationStatus, SolarPhase, SolarStatus};
use sunrise::{Coordinates as SunriseCoordinates, SolarDay, SolarEvent};
use tokio::time;

use crate::{
    context::{Ctx, SourceGuard},
    publisher::Publisher,
    service::{Input, Service, ServiceError},
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
    _tick: SourceGuard,
    _on_location: SourceGuard,
}

impl Service for Solar {
    type Config = ();
    type Command = Command;
    type Event = Event;

    async fn start(ctx: &Ctx<Self>, _config: Self::Config) -> Result<Self, ServiceError> {
        tracing::debug!("starting solar service");
        Ok(Self {
            coordinates: None,
            status: ctx.publisher::<SolarStatus>(),
            _tick: ctx.interval(TICK, || Event::Tick),
            _on_location: ctx
                .subscribe::<GeolocationStatus>(move |data| Event::Update(data.coordinates)),
        })
    }

    async fn handle(&mut self, _ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Update(coordinates)) => {
                self.coordinates = coordinates;
            }
            Input::Event(Event::Tick) | Input::Command(Command::Refresh) => {
                match self.coordinates {
                    Some(ref coordinates) => {
                        if let Some(times) =
                            solar_times_for_date(chrono::Local::now().date_naive(), coordinates)
                        {
                            self.status.set(SolarStatus {
                                phase: detect_phase(&times),
                            });
                        }
                    }
                    None => {
                        tracing::warn!("solar: location unavailable")
                    }
                }
            }
            Input::Config(()) => {}
        }
    }

    fn peek_config(config: &glimpse_config::Config) -> Self::Config {
        ()
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
