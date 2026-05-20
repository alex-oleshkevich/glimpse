use glimpse_core::services::weather::model::{CurrentWeather, DailyForecast, Location, State};

pub fn hero_summary(current: &CurrentWeather) -> String {
    format!(
        "{} · Feels like {}",
        current.condition,
        temperature(current.apparent_temperature)
    )
}

pub fn build_detail_rows(
    current: &CurrentWeather,
    today: Option<&DailyForecast>,
) -> Vec<(String, String)> {
    let high = today
        .map(|d| temperature(d.temperature_max))
        .unwrap_or_else(|| "—".into());
    let low = today
        .map(|d| temperature(d.temperature_min))
        .unwrap_or_else(|| "—".into());
    let sunrise = today
        .map(|d| display_time_or_dash(&d.sunrise))
        .unwrap_or_else(|| "—".into());
    let sunset = today
        .map(|d| display_time_or_dash(&d.sunset))
        .unwrap_or_else(|| "—".into());

    vec![
        ("High".into(), high),
        ("Low".into(), low),
        ("Humidity".into(), format!("{}%", current.humidity)),
        (
            "Wind".into(),
            format!(
                "{} {:.0} km/h",
                current.wind_direction_label, current.wind_speed
            ),
        ),
        ("Pressure".into(), format!("{:.0} hPa", current.pressure)),
        ("UV".into(), format!("{:.0}", current.uv_index)),
        ("Sunrise".into(), sunrise),
        ("Sunset".into(), sunset),
    ]
}

pub fn display_time_or_dash(value: &str) -> String {
    value
        .split('T')
        .nth(1)
        .filter(|v| !v.is_empty())
        .unwrap_or("—")
        .to_owned()
}

pub fn forecast_items(items: &[DailyForecast]) -> Vec<DailyForecast> {
    items
        .iter()
        .filter(|item| !item.is_today)
        .cloned()
        .collect()
}

pub const DEFAULT_LABEL_FORMAT: &str = "{temp}";
pub const DEFAULT_TOOLTIP_FORMAT: &str =
    "{condition} · {temp} · feels like {feels_like} · {location}";

pub fn icon_name(state: &State) -> String {
    match state {
        State::Ready(snapshot) => snapshot.current.icon.clone(),
        State::Loading | State::Unknown | State::Unavailable(_) => {
            "weather-overcast-symbolic".into()
        }
    }
}

pub fn label(template: &str, state: &State) -> String {
    let State::Ready(snapshot) = state else {
        return String::new();
    };

    text(template, &snapshot.current, &snapshot.location)
}

pub fn tooltip(template: &str, state: &State) -> String {
    match state {
        State::Ready(snapshot) => text(template, &snapshot.current, &snapshot.location),
        State::Loading => "Loading weather".into(),
        State::Unavailable(message) if !message.is_empty() => message.clone(),
        State::Unknown | State::Unavailable(_) => "Weather".into(),
    }
}

pub fn text(template: &str, current: &CurrentWeather, location: &Location) -> String {
    template
        .replace("{temp}", &format!("{:.0}°", current.temperature))
        .replace("{condition}", &current.condition)
        .replace(
            "{feels_like}",
            &format!("{:.0}°", current.apparent_temperature),
        )
        .replace("{location}", &location.city)
}

pub fn temperature(value: f64) -> String {
    format!("{value:.0}°")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_replaces_weather_placeholders() {
        let current = CurrentWeather {
            temperature: 19.6,
            apparent_temperature: 18.2,
            condition: "Cloudy".into(),
            ..CurrentWeather::default()
        };
        let location = Location {
            city: "Warsaw, PL".into(),
            ..Location::default()
        };

        assert_eq!(
            text(
                "{condition} · {temp} · {feels_like} · {location}",
                &current,
                &location,
            ),
            "Cloudy · 20° · 18° · Warsaw, PL"
        );
    }

    #[test]
    fn hero_summary_formats_condition_and_feels_like() {
        let current = CurrentWeather {
            condition: "Overcast".into(),
            apparent_temperature: 9.0,
            ..CurrentWeather::default()
        };
        assert_eq!(hero_summary(&current), "Overcast · Feels like 9°");
    }

    #[test]
    fn build_detail_rows_returns_eight_items() {
        let current = CurrentWeather {
            humidity: 82,
            wind_speed: 18.0,
            wind_direction_label: "NW".into(),
            pressure: 1008.0,
            uv_index: 1.0,
            ..CurrentWeather::default()
        };
        let today = DailyForecast {
            temperature_min: 8.0,
            temperature_max: 14.0,
            sunrise: "2099-01-01T06:12".into(),
            sunset: "2099-01-01T19:48".into(),
            ..DailyForecast::default()
        };
        let rows = build_detail_rows(&current, Some(&today));
        assert_eq!(rows.len(), 8);
        assert_eq!(rows[0].1, "14°");
        assert_eq!(rows[1].1, "8°");
        assert_eq!(rows[6].1, "06:12");
        assert_eq!(rows[7].1, "19:48");
    }

    #[test]
    fn display_time_or_dash_extracts_time_part() {
        assert_eq!(display_time_or_dash("2099-01-01T06:12"), "06:12");
        assert_eq!(display_time_or_dash(""), "—");
    }

    #[test]
    fn forecast_items_start_after_today() {
        let today = DailyForecast {
            is_today: true,
            ..DailyForecast::default()
        };
        let tomorrow = DailyForecast {
            day_name: "Fri".into(),
            ..DailyForecast::default()
        };
        assert_eq!(forecast_items(&[today, tomorrow.clone()]), vec![tomorrow]);
    }
}
