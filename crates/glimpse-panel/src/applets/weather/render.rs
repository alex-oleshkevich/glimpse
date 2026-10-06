use chrono::{DateTime, FixedOffset, Offset as _, TimeDelta, Utc};
use gettextrs::{gettext, ngettext, pgettext};
use glimpse_dbus::weather::{
    AlertSeverity, Condition, CurrentWeather, DayForecast, PlaceWeather, UnitSystem, WeatherAlert,
    reading, rounded,
};
use glimpse_widgets::{Advisory, Day, Fact, Hour, Severity, WeatherPage, alert_page, day_page};

pub const ALERT_ICON: &str = "dialog-warning-symbolic";
/// How far ahead the nowcast looks. Beyond it a wet hour is tomorrow's weather rather than
/// something worth interrupting the popover for.
const NOWCAST: i64 = 180;

pub fn wording(condition: Condition) -> Option<String> {
    Some(match condition {
        Condition::ClearSky => gettext("Clear"),
        Condition::MainlyClear => gettext("Mainly clear"),
        Condition::PartlyCloudy => gettext("Partly cloudy"),
        Condition::Overcast => gettext("Overcast"),
        Condition::Fog => gettext("Fog"),
        Condition::Drizzle => gettext("Drizzle"),
        Condition::FreezingDrizzle => gettext("Freezing drizzle"),
        Condition::LightRain => gettext("Light rain"),
        Condition::Rain => gettext("Rain"),
        Condition::HeavyRain => gettext("Heavy rain"),
        Condition::FreezingRain => gettext("Freezing rain"),
        Condition::LightSnow => gettext("Light snow"),
        Condition::Snow => gettext("Snow"),
        Condition::HeavySnow => gettext("Heavy snow"),
        Condition::SnowGrains => gettext("Snow grains"),
        Condition::Sleet => gettext("Sleet"),
        Condition::RainShowers => gettext("Rain showers"),
        Condition::SnowShowers => gettext("Snow showers"),
        Condition::Thunderstorm => gettext("Thunderstorm"),
        Condition::ThunderstormWithHail => gettext("Thunderstorm with hail"),
        Condition::Unknown => return None,
    })
}

fn wet(condition: Condition) -> bool {
    match condition {
        Condition::Drizzle
        | Condition::FreezingDrizzle
        | Condition::LightRain
        | Condition::Rain
        | Condition::HeavyRain
        | Condition::FreezingRain
        | Condition::LightSnow
        | Condition::Snow
        | Condition::HeavySnow
        | Condition::SnowGrains
        | Condition::Sleet
        | Condition::RainShowers
        | Condition::SnowShowers
        | Condition::Thunderstorm
        | Condition::ThunderstormWithHail => true,
        Condition::ClearSky
        | Condition::MainlyClear
        | Condition::PartlyCloudy
        | Condition::Overcast
        | Condition::Fog
        | Condition::Unknown => false,
    }
}

fn frozen(condition: Condition) -> bool {
    matches!(
        condition,
        Condition::LightSnow
            | Condition::Snow
            | Condition::HeavySnow
            | Condition::SnowGrains
            | Condition::SnowShowers
    )
}

pub fn zone(seconds: i32) -> FixedOffset {
    FixedOffset::east_opt(seconds).unwrap_or_else(|| Utc.fix())
}

pub fn subtitle(current: &CurrentWeather) -> Option<String> {
    let condition = wording(current.condition);
    let feels = current.apparent_temperature.map(|apparent| {
        gettext("feels like {temperature}").replace("{temperature}", &reading(apparent))
    });

    match (condition, feels) {
        (Some(condition), Some(feels)) => Some(format!("{condition} · {feels}")),
        (Some(only), None) | (None, Some(only)) => Some(only),
        (None, None) => None,
    }
}

/// The strip starts at the next hour. The wire opens at the hour standing, and the hero is already
/// showing it — a column repeating it costs one of the hours the strip exists to look ahead at.
pub fn hours(place: &PlaceWeather, cap: u8, twelve: bool) -> Vec<Hour> {
    let offset = zone(place.utc_offset_seconds);
    let clock = glimpse_config::clock(twelve);

    place
        .hours
        .iter()
        .skip(1)
        .take(cap as usize)
        .map(|hour| Hour {
            label: hour.time.with_timezone(&offset).format(clock).to_string(),
            icon_name: hour.condition.icon_name(hour.is_day).to_owned(),
            temperature: hour.temperature,
            now: false,
        })
        .collect()
}

pub fn days(place: &PlaceWeather, cap: u8) -> Vec<Day> {
    let offset = zone(place.utc_offset_seconds);
    let now = place.current.as_ref().map(|current| current.temperature);

    place
        .days
        .iter()
        .take(cap as usize + 1)
        .enumerate()
        .map(|(index, day)| Day {
            label: day_label(index, day, offset),
            icon_name: day.condition.icon_name(true).to_owned(),
            precipitation: day.precipitation_chance.map(u32::from),
            low: day.low,
            high: day.high,
            now: now.filter(|_| index == 0),
        })
        .collect()
}

fn day_label(index: usize, day: &DayForecast, offset: FixedOffset) -> String {
    match index {
        0 => gettext("Today"),
        1 => gettext("Tomorrow"),
        _ => weekday(day.start.with_timezone(&offset)),
    }
}

/// A weekday name belongs to `LC_TIME`, which `init_translations` has already set, rather than to
/// the message catalog — so it is formatted rather than looked up.
fn weekday(when: DateTime<FixedOffset>) -> String {
    glib::DateTime::from_unix_utc(when.timestamp())
        .and_then(|utc| {
            utc.to_timezone(&glib::TimeZone::from_offset(
                when.offset().local_minus_utc(),
            ))
        })
        .and_then(|local| local.format("%a"))
        .map(|name| name.to_string())
        .unwrap_or_else(|_| when.format("%a").to_string())
}

pub fn severity(severity: AlertSeverity) -> Severity {
    match severity {
        AlertSeverity::Minor | AlertSeverity::Unknown => Severity::Info,
        AlertSeverity::Moderate => Severity::Warning,
        AlertSeverity::Severe | AlertSeverity::Extreme => Severity::Error,
    }
}

/// The chip has room for one condition, so it reports the worst thing standing rather than the
/// first one the provider happened to list.
pub fn worst(alerts: &[WeatherAlert]) -> Option<Severity> {
    alerts
        .iter()
        .map(|alert| severity(alert.severity))
        .max_by_key(|severity| match severity {
            Severity::Info => 0,
            Severity::Warning => 1,
            Severity::Error => 2,
        })
}

pub fn alerts(place: &PlaceWeather) -> Vec<Advisory> {
    place
        .alerts
        .iter()
        .enumerate()
        .map(|(index, alert)| Advisory {
            severity: severity(alert.severity),
            icon_name: ALERT_ICON.to_owned(),
            title: alert.headline.clone(),
            subtitle: alert.source.clone(),
            page: Some(alert_page(index)),
        })
        .collect()
}

/// The first wet hour ahead, when the hour standing is dry. Already being rained on is not news.
pub fn nowcast(place: &PlaceWeather, now: DateTime<Utc>) -> Option<Advisory> {
    let current = place.current.as_ref()?;
    if wet(current.condition) {
        return None;
    }

    let (minutes, hour) = place.hours.iter().find_map(|hour| {
        let minutes = (hour.time - now).num_minutes();
        (wet(hour.condition) && (0..=NOWCAST).contains(&minutes)).then_some((minutes, hour))
    })?;

    let what = match frozen(hour.condition) {
        true => gettext("Snow"),
        false => gettext("Rain"),
    };
    let minutes = minutes.max(0) as u32;
    let (count, title) = match minutes {
        0..60 => (
            minutes,
            ngettext(
                "{what} starting in {count} minute",
                "{what} starting in {count} minutes",
                minutes,
            ),
        ),
        _ => {
            let hours = (minutes + 30) / 60;
            (
                hours,
                ngettext(
                    "{what} starting in {count} hour",
                    "{what} starting in {count} hours",
                    hours,
                ),
            )
        }
    };

    Some(Advisory {
        severity: Severity::Info,
        icon_name: hour.condition.icon_name(hour.is_day).to_owned(),
        title: title
            .replace("{what}", &what)
            .replace("{count}", &count.to_string()),
        subtitle: wording(hour.condition),
        page: None,
    })
}

fn temperature_unit(units: UnitSystem) -> String {
    match units {
        UnitSystem::Metric => gettext("°C"),
        UnitSystem::Imperial => gettext("°F"),
    }
}

fn fact(label: String, value: String) -> Fact {
    Fact::new(label, value)
}

fn clock_at(when: DateTime<Utc>, offset: FixedOffset, twelve: bool) -> String {
    let pattern = glimpse_config::clock(twelve);
    when.with_timezone(&offset).format(pattern).to_string()
}

pub fn day_length(day: &DayForecast) -> Option<String> {
    let (sunrise, sunset) = (day.sunrise?, day.sunset?);
    let span = sunset - sunrise;
    if span <= TimeDelta::zero() {
        return None;
    }
    Some(
        gettext("{hours} h {minutes} min")
            .replace("{hours}", &span.num_hours().to_string())
            .replace("{minutes}", &(span.num_minutes() % 60).to_string()),
    )
}

fn compass(degrees: u16) -> String {
    match (u32::from(degrees) * 2 + 45) / 90 % 8 {
        0 => pgettext("wind", "N"),
        1 => pgettext("wind", "NE"),
        2 => pgettext("wind", "E"),
        3 => pgettext("wind", "SE"),
        4 => pgettext("wind", "S"),
        5 => pgettext("wind", "SW"),
        6 => pgettext("wind", "W"),
        _ => pgettext("wind", "NW"),
    }
}

fn wind(current: &CurrentWeather, units: UnitSystem) -> Option<String> {
    let speed = current.wind_speed?;
    let unit = match units {
        UnitSystem::Metric => gettext("km/h"),
        UnitSystem::Imperial => gettext("mph"),
    };
    let speed = format!("{} {unit}", rounded(speed));
    Some(match current.wind_direction {
        Some(degrees) => format!("{speed} {}", compass(degrees)),
        None => speed,
    })
}

fn precipitation(amount: f64, units: UnitSystem) -> String {
    match units {
        UnitSystem::Metric => gettext("{amount} mm").replace("{amount}", &format!("{amount:.1}")),
        UnitSystem::Imperial => gettext("{amount} in").replace("{amount}", &format!("{amount:.2}")),
    }
}

fn current_facts(current: &CurrentWeather, units: UnitSystem) -> Vec<Fact> {
    let mut facts = Vec::new();
    if let Some(wind) = wind(current, units) {
        facts.push(fact(gettext("Wind"), wind));
    }
    if let Some(humidity) = current.humidity {
        facts.push(fact(gettext("Humidity"), format!("{humidity}%")));
    }
    if let Some(amount) = current.precipitation {
        facts.push(fact(gettext("Precipitation"), precipitation(amount, units)));
    }
    facts
}

fn day_facts(day: &DayForecast, units: UnitSystem, offset: FixedOffset, twelve: bool) -> Vec<Fact> {
    let mut facts = vec![
        fact(
            gettext("High"),
            format!("{} {}", rounded(day.high), temperature_unit(units)),
        ),
        fact(
            gettext("Low"),
            format!("{} {}", rounded(day.low), temperature_unit(units)),
        ),
    ];

    if let Some(chance) = day.precipitation_chance {
        facts.push(fact(gettext("Chance of rain"), format!("{chance}%")));
    }
    facts.extend(sun_facts(day, offset, twelve));
    facts
}

fn sun_facts(day: &DayForecast, offset: FixedOffset, twelve: bool) -> Vec<Fact> {
    let mut facts = Vec::new();
    if let Some(sunrise) = day.sunrise {
        facts.push(fact(gettext("Sunrise"), clock_at(sunrise, offset, twelve)));
    }
    if let Some(sunset) = day.sunset {
        facts.push(fact(gettext("Sunset"), clock_at(sunset, offset, twelve)));
    }
    if let Some(length) = day_length(day) {
        facts.push(fact(gettext("Day length"), length));
    }
    facts
}

pub fn pages(place: &PlaceWeather, units: UnitSystem, cap: u8, twelve: bool) -> Vec<WeatherPage> {
    let offset = zone(place.utc_offset_seconds);
    let mut pages = Vec::new();

    pages.extend(
        place
            .days
            .iter()
            .take(cap as usize + 1)
            .enumerate()
            .map(|(index, day)| WeatherPage {
                key: day_page(index as u32),
                title: day_label(index, day, offset),
                description: wording(day.condition),
                facts: match (index, place.current.as_ref()) {
                    (0, Some(current)) => current_facts(current, units)
                        .into_iter()
                        .chain(sun_facts(day, offset, twelve))
                        .collect(),
                    _ => day_facts(day, units, offset, twelve),
                },
            }),
    );

    pages.extend(
        place
            .alerts
            .iter()
            .enumerate()
            .map(|(index, alert)| WeatherPage {
                key: alert_page(index),
                title: alert.headline.clone(),
                description: alert.description.clone(),
                facts: Vec::new(),
            }),
    );

    pages
}

pub fn tooltip(format: &str, place: &str, current: &CurrentWeather) -> String {
    let feels = current
        .apparent_temperature
        .map(reading)
        .unwrap_or_else(|| reading(current.temperature));

    format
        .replace("{place}", place)
        .replace("{temperature}", &reading(current.temperature))
        .replace("{feels_like}", &feels)
        .replace(
            "{condition}",
            &wording(current.condition).unwrap_or_default(),
        )
}

#[cfg(test)]
mod tests;
