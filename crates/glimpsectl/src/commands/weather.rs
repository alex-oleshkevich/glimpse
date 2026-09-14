use anyhow::{Context, Result};
use glimpse_dbus::weather::{Condition, PlaceWeather, UnitSystem, WatchedPlace, WeatherStatus};
use glimpse_dbus::weather::{Weather1Proxy, decode_snapshot};
use serde::Serialize;
use zbus::Connection;

use super::{ABSENT, emit, proxy, reason_or_absent, safe, within, yes_no};
use crate::render::{Section, Table, styled};

#[derive(Serialize)]
struct Report<'a> {
    available: bool,
    stale: bool,
    reason: Option<&'a str>,
    status: Option<&'a WeatherStatus>,
}

pub async fn weather_status(connection: &Connection, json: bool) -> Result<()> {
    let snapshot = within(proxy::<Weather1Proxy>(connection).await?.snapshot())
        .await
        .context("cannot read the weather")?;
    let state = decode_snapshot(snapshot)
        .map_err(anyhow::Error::msg)
        .context("the weather provider sent a snapshot this build cannot read")?;

    if json {
        return emit(&Report {
            available: state.available,
            stale: state.stale,
            reason: state.reason.as_deref(),
            status: state.status.as_ref(),
        });
    }

    let units = state.status.as_ref().map(|status| status.units);
    let header = Section::new("Weather").with(
        Table::new()
            .with_row(["available".to_owned(), yes_no(state.available).to_owned()])
            .with_row(["stale".to_owned(), yes_no(state.stale).to_owned()])
            .with_row([
                "reason".to_owned(),
                styled::key(&reason_or_absent(&safe(
                    state.reason.as_deref().unwrap_or(""),
                ))),
            ])
            .with_row([
                "updated".to_owned(),
                state
                    .status
                    .as_ref()
                    .and_then(|status| status.updated_at)
                    .map_or_else(|| ABSENT.to_owned(), |at| at.to_rfc3339()),
            ])
            .render(),
    );
    header.print()?;

    crate::render::print("")?;
    Section::new("Places")
        .with(
            Table::new()
                .with_headers(["PLACE", "TEMP", "CONDITION", "ALERTS"])
                .with_empty("nothing is watching a place, so nothing is fetched")
                .with_rows(
                    state
                        .status
                        .iter()
                        .flat_map(|status| status.places.iter())
                        .map(|place| row(place, units)),
                )
                .render(),
        )
        .print()
}

pub async fn weather_refresh(connection: &Connection) -> Result<()> {
    within(proxy::<Weather1Proxy>(connection).await?.refresh())
        .await
        .context("cannot refresh the weather")?;
    Ok(())
}

fn row(place: &PlaceWeather, units: Option<UnitSystem>) -> [String; 4] {
    let (temperature, condition) = match &place.current {
        Some(current) => (
            format!("{:.0}{}", current.temperature, degrees(units)),
            condition(current.condition).to_owned(),
        ),
        None => (ABSENT.to_owned(), ABSENT.to_owned()),
    };
    [
        name(place),
        temperature,
        condition,
        alerts(place.alerts.len()),
    ]
}

fn name(place: &PlaceWeather) -> String {
    if let Some(city) = &place.city {
        return match &place.country_code {
            Some(code) => format!("{city}, {code}"),
            None => city.clone(),
        };
    }
    match &place.place {
        WatchedPlace::Here => "here".to_owned(),
        WatchedPlace::Location { name } => name.clone(),
        WatchedPlace::Coordinates {
            latitude,
            longitude,
        } => format!("{latitude:.3}, {longitude:.3}"),
    }
}

fn alerts(count: usize) -> String {
    match count {
        0 => ABSENT.to_owned(),
        count => styled::warn(&count.to_string()),
    }
}

fn degrees(units: Option<UnitSystem>) -> &'static str {
    match units {
        Some(UnitSystem::Imperial) => "°F",
        _ => "°C",
    }
}

fn condition(condition: Condition) -> &'static str {
    match condition {
        Condition::ClearSky => "clear",
        Condition::MainlyClear => "mainly clear",
        Condition::PartlyCloudy => "partly cloudy",
        Condition::Overcast => "overcast",
        Condition::Fog => "fog",
        Condition::Drizzle => "drizzle",
        Condition::FreezingDrizzle => "freezing drizzle",
        Condition::LightRain => "light rain",
        Condition::Rain => "rain",
        Condition::HeavyRain => "heavy rain",
        Condition::FreezingRain => "freezing rain",
        Condition::LightSnow => "light snow",
        Condition::Snow => "snow",
        Condition::HeavySnow => "heavy snow",
        Condition::SnowGrains => "snow grains",
        Condition::Sleet => "sleet",
        Condition::RainShowers => "rain showers",
        Condition::SnowShowers => "snow showers",
        Condition::Thunderstorm => "thunderstorm",
        Condition::ThunderstormWithHail => "thunderstorm with hail",
        Condition::Unknown => ABSENT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_dbus::weather::GeoCoordinates;

    fn place(place: WatchedPlace) -> PlaceWeather {
        PlaceWeather {
            place,
            coordinates: GeoCoordinates {
                latitude: 52.52,
                longitude: 13.405,
            },
            city: None,
            country_code: None,
            utc_offset_seconds: 0,
            current: None,
            hours: Vec::new(),
            days: Vec::new(),
            alerts: Vec::new(),
        }
    }

    #[test]
    fn a_resolved_place_reads_as_its_city_and_country() {
        let mut resolved = place(WatchedPlace::Here);
        resolved.city = Some("Berlin".to_owned());
        resolved.country_code = Some("DE".to_owned());
        assert_eq!(name(&resolved), "Berlin, DE");
    }

    #[test]
    fn an_unresolved_place_falls_back_to_what_was_asked_for() {
        assert_eq!(name(&place(WatchedPlace::Here)), "here");
        assert_eq!(
            name(&place(WatchedPlace::Location {
                name: "Berlin, DE".to_owned()
            })),
            "Berlin, DE"
        );
        assert_eq!(
            name(&place(WatchedPlace::Coordinates {
                latitude: 52.52,
                longitude: 13.405
            })),
            "52.520, 13.405"
        );
    }

    #[test]
    fn a_place_with_no_reading_says_so_rather_than_printing_a_zero() {
        let [_, temperature, condition, alerts] = row(&place(WatchedPlace::Here), None);
        assert_eq!(temperature, ABSENT);
        assert_eq!(condition, ABSENT);
        assert_eq!(alerts, ABSENT);
    }

    #[test]
    fn units_decide_the_degree_symbol() {
        assert_eq!(degrees(Some(UnitSystem::Imperial)), "°F");
        assert_eq!(degrees(Some(UnitSystem::Metric)), "°C");
        assert_eq!(degrees(None), "°C");
    }
}
