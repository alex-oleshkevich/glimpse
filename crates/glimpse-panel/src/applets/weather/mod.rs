pub(crate) mod render;

use std::time::Duration;

use chrono::Utc;
use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind, WeatherAppletConfig, WeatherPlace};
use glimpse_dbus::weather::{Condition, GeoCoordinates, PlaceWeather, UnitSystem, WatchedPlace};
use glimpse_dbus::weather::{WeatherProviderHandle, WeatherProviderState};
use glimpse_widgets::{IndicatorSpec, Severity, WeatherPopover};
use gtk4::{gio, glib, prelude::*};

use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{Applet, Ctx, Input, spawn_command};

const MINUTE: Duration = Duration::from_secs(60);

pub struct Weather {
    weather: WeatherProviderHandle,
    settings: WeatherAppletConfig,
    watching: WatchedPlace,
    units: UnitSystem,
    place: Option<PlaceWeather>,
    twelve: bool,
    tooltip_format: Option<String>,
    footer: Option<(String, Vec<String>)>,
    icon: Option<(String, gio::Icon)>,
    spec: Vec<IndicatorSpec>,
    shown: glib::WeakRef<WeatherPopover>,
    owner: bool,
    stale: bool,
    trouble: Option<String>,
    unserved: bool,
}

impl Applet for Weather {
    fn configure(&mut self, ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Weather(settings) = &config.kind else {
            return;
        };
        let watching = watched(&settings.place);
        if watching != self.watching {
            self.place = None;
            self.unserved = false;
        }
        self.watching = watching;
        self.settings = settings.clone();
        self.twelve = config.regional.twelve_hour();
        self.tooltip_format = config.common.tooltip_format.clone();
        self.footer = config
            .common
            .settings()
            .map(|(label, command)| (label.to_owned(), command.to_vec()));

        ctx.interval(MINUTE);
        self.renew();
        self.sync();
        self.refresh();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        match input {
            Input::Tick => {
                self.note_unserved();
                self.renew();
            }
            Input::Woken => {
                let had_owner = self.owner;
                self.sync();
                if !had_owner && self.owner {
                    self.renew();
                }
            }
            _ => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, _seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = WeatherPopover::new();

        if let Some((_, command)) = &self.footer {
            let command = command.clone();
            shown.connect_footer_activated(move |_| run(&command));
        }

        self.shown.set(Some(&shown));
        self.refresh();
        Some(Box::new(shown))
    }
}

impl Weather {
    pub fn start(weather: WeatherProviderHandle) -> Self {
        let mut service = Self {
            weather,
            settings: WeatherAppletConfig::default(),
            watching: WatchedPlace::Here,
            units: UnitSystem::Metric,
            place: None,
            twelve: false,
            tooltip_format: None,
            footer: None,
            icon: None,
            spec: Vec::new(),
            shown: glib::WeakRef::new(),
            owner: false,
            stale: false,
            trouble: None,
            unserved: false,
        };
        service.sync();
        service
    }

    /// A fixed place still missing a whole tick after the provider took the name is one the
    /// provider is not serving, which is worth a chip. A place that has simply not been fetched yet
    /// is missing for well under a tick, and `here` may legitimately never resolve.
    fn note_unserved(&mut self) {
        self.unserved = self.owner && self.place.is_none() && self.watching != WatchedPlace::Here;
    }

    fn renew(&self) {
        let weather = self.weather.clone();
        let place = self.watching.clone();
        spawn_command("weather.watch", async move { weather.watch(place).await });
    }

    fn sync(&mut self) {
        let WeatherProviderState {
            status,
            stale,
            reason,
            owner,
            ..
        } = self.weather.snapshot();
        self.owner = owner;
        self.stale = stale;
        self.trouble = reason;
        match status {
            Some(status) => {
                self.units = status.units;
                self.place = status
                    .places
                    .into_iter()
                    .find(|place| place.place == self.watching);
            }
            None => self.place = None,
        }
    }

    fn refresh(&mut self) {
        self.spec = self.indicator().into_iter().collect();

        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    /// Nothing to show is an empty `Vec`, so a panel with no fix yet renders no chip rather than a
    /// placeholder — and a `here` place with no fix and a place with no reading collapse to it.
    /// An alert takes the chip's icon and its colour. The bar has room for one thing, and a
    /// warning that is standing outranks the condition it is standing in.
    fn indicator(&mut self) -> Option<IndicatorSpec> {
        let Some(place) = self.place.as_ref() else {
            return self.trouble_indicator();
        };
        let Some(current) = place.current.clone() else {
            return self.trouble_indicator();
        };
        let mut severity = render::worst(&place.alerts);
        if self.stale && severity != Some(Severity::Error) {
            severity = Some(Severity::Warning);
        }
        let name = match severity.is_some() {
            true => render::ALERT_ICON,
            false => render::icon(current.condition, current.is_day),
        };
        let icon = self.themed(name);
        let label = self.label();

        Some(IndicatorSpec {
            icon: Some(icon),
            label: Some(render::reading(current.temperature)),
            tooltip: self.trouble.clone().or_else(|| {
                self.tooltip_format
                    .as_deref()
                    .map(|format| render::tooltip(format, &label, &current))
            }),
            severity,
            ..Default::default()
        })
    }

    fn trouble_indicator(&mut self) -> Option<IndicatorSpec> {
        let reason = self.trouble.clone().or_else(|| {
            self.unserved
                .then(|| gettext("The weather provider is not serving this place."))
        })?;
        Some(IndicatorSpec {
            icon: Some(self.themed(render::icon(Condition::Unknown, true))),
            tooltip: Some(reason),
            severity: Some(Severity::Warning),
            ..Default::default()
        })
    }

    /// `indicators` is pulled after every input, so the icon is rebuilt when the name changes
    /// rather than once per render.
    fn themed(&mut self, name: &str) -> gio::Icon {
        if self.icon.as_ref().is_none_or(|(held, _)| held != name) {
            let icon = gio::ThemedIcon::new(name).upcast();
            self.icon = Some((name.to_owned(), icon));
        }

        match self.icon.as_ref() {
            Some((_, icon)) => icon.clone(),
            None => gio::ThemedIcon::new(name).upcast(),
        }
    }

    fn label(&self) -> String {
        if let Some(label) = self.settings.label.as_deref() {
            return label.to_owned();
        }
        let Some(place) = self.place.as_ref() else {
            return gettext("Here");
        };
        match (&place.city, &place.country_code) {
            (Some(city), Some(country_code)) => format!("{city}, {country_code}"),
            (Some(city), None) => city.clone(),
            (None, Some(country_code)) => country_code.clone(),
            (None, None) => pair(&place.coordinates),
        }
    }

    fn dress(&self, shown: &WeatherPopover) {
        shown.set_footer(self.footer.as_ref().map(|(label, _)| label.as_str()));

        let Some(place) = self.place.as_ref() else {
            shown.set_heading(
                render::icon(Condition::Unknown, true),
                &self.label(),
                self.trouble.as_deref().or(Some(&gettext("No reading yet"))),
            );
            shown.set_reading(None);
            shown.set_hours(&[]);
            shown.set_days(&[]);
            shown.set_nowcast(None);
            shown.set_alerts(&[]);
            shown.set_pages(&[]);
            return;
        };

        let (hours, days) = (self.settings.hours, self.settings.days);
        shown.set_hours(&render::hours(place, hours, self.twelve));
        shown.set_days(&render::days(place, days));
        shown.set_nowcast(render::nowcast(place, Utc::now()).as_ref());
        shown.set_alerts(&render::alerts(place));
        shown.set_pages(&render::pages(place, self.units, days, self.twelve));

        match place.current.as_ref() {
            Some(current) => {
                shown.set_heading(
                    render::icon(current.condition, current.is_day),
                    &self.label(),
                    self.trouble
                        .as_deref()
                        .or(render::subtitle(current).as_deref()),
                );
                shown.set_reading(Some((
                    &render::rounded(current.temperature),
                    render::DEGREE,
                )));
            }
            None => {
                shown.set_heading(
                    render::icon(Condition::Unknown, true),
                    &self.label(),
                    self.trouble.as_deref().or(Some(&gettext("No reading yet"))),
                );
                shown.set_reading(None);
            }
        }
    }
}

fn watched(place: &WeatherPlace) -> WatchedPlace {
    match place {
        WeatherPlace::Here {} => WatchedPlace::Here,
        WeatherPlace::Coordinates {
            latitude,
            longitude,
        } => WatchedPlace::Coordinates {
            latitude: *latitude,
            longitude: *longitude,
        },
        WeatherPlace::Location { name } => WatchedPlace::Location { name: name.clone() },
    }
}

fn pair(coordinates: &GeoCoordinates) -> String {
    format!("{:.2}, {:.2}", coordinates.latitude, coordinates.longitude)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;
    use glimpse_dbus::weather::WeatherProvider;
    use glimpse_dbus::weather::{AlertSeverity, CurrentWeather, GeoCoordinates, WeatherAlert};
    use glimpse_widgets::Severity;

    use super::*;

    fn applet() -> Weather {
        let provider = WeatherProvider::unavailable("test provider unavailable");
        let mut applet = Weather::start(provider.handle());
        applet.trouble = None;
        applet
    }

    #[test]
    fn an_unavailable_provider_renders_a_warning() {
        let provider = WeatherProvider::unavailable("weather provider unavailable");
        let mut applet = Weather::start(provider.handle());

        applet.refresh();

        let indicators = applet.indicators();
        assert_eq!(indicators.len(), 1);
        assert_eq!(indicators[0].severity, Some(Severity::Warning));
        assert_eq!(
            indicators[0].tooltip.as_deref(),
            Some("weather provider unavailable")
        );
    }

    #[test]
    fn stale_weather_keeps_its_reading_and_renders_a_warning() {
        let mut applet = applet();
        let mut place = reading();
        place.alerts.push(WeatherAlert {
            severity: AlertSeverity::Minor,
            headline: "minor alert".to_owned(),
            description: None,
            source: None,
            starts_at: None,
            expires_at: None,
        });
        applet.place = Some(place);
        applet.stale = true;
        applet.trouble = Some("weather provider unavailable".to_owned());

        applet.refresh();

        let indicators = applet.indicators();
        assert_eq!(indicators.len(), 1);
        assert_eq!(indicators[0].severity, Some(Severity::Warning));
        assert!(indicators[0].label.is_some());
        assert_eq!(
            indicators[0].tooltip.as_deref(),
            Some("weather provider unavailable")
        );
    }

    #[test]
    fn a_fixed_place_the_provider_never_serves_is_reported_on_the_tick_and_not_before() {
        let mut applet = applet();
        applet.watching = WatchedPlace::Coordinates {
            latitude: 54.6872,
            longitude: 25.2797,
        };
        applet.owner = true;
        applet.place = None;

        applet.refresh();
        assert!(
            applet.indicators().is_empty(),
            "the gap before the first fetch renders nothing, as it always has"
        );

        applet.note_unserved();
        applet.refresh();

        let indicators = applet.indicators();
        assert_eq!(indicators.len(), 1);
        assert_eq!(indicators[0].severity, Some(Severity::Warning));
        assert_eq!(
            indicators[0].tooltip.as_deref(),
            Some("The weather provider is not serving this place.")
        );
    }

    /// The flag is the tick's to set. A snapshot arriving must not decide it, or the ordinary gap
    /// between taking the name and the first fetch would flash a warning on every panel start.
    #[test]
    fn a_snapshot_does_not_decide_whether_a_place_is_unserved() {
        let mut applet = applet();
        applet.unserved = true;

        applet.sync();

        assert!(
            applet.unserved,
            "sync reads the provider; only the tick judges how long a place has been missing"
        );
    }

    #[test]
    fn here_without_a_fix_stays_silent_however_long_it_waits() {
        let mut applet = applet();
        applet.owner = true;
        applet.place = None;

        applet.note_unserved();
        applet.refresh();

        assert!(
            applet.indicators().is_empty(),
            "`here` may legitimately never resolve, so it never becomes a warning"
        );
    }

    #[test]
    fn a_place_that_arrives_clears_the_warning() {
        let mut applet = applet();
        applet.watching = WatchedPlace::Here;
        applet.owner = true;
        applet.note_unserved();
        applet.place = Some(reading());

        applet.note_unserved();
        applet.refresh();

        let indicators = applet.indicators();
        assert_eq!(indicators.len(), 1);
        assert_eq!(indicators[0].severity, None);
    }

    fn reading() -> PlaceWeather {
        PlaceWeather {
            place: WatchedPlace::Here,
            coordinates: GeoCoordinates {
                latitude: 54.6872,
                longitude: 25.2797,
            },
            city: Some("Vilnius".to_owned()),
            country_code: Some("LT".to_owned()),
            utc_offset_seconds: 10_800,
            current: Some(CurrentWeather {
                observed_at: Utc.with_ymd_and_hms(2026, 9, 8, 12, 0, 0).unwrap(),
                condition: Condition::LightRain,
                is_day: true,
                temperature: 18.4,
                apparent_temperature: Some(16.2),
                humidity: Some(72),
                wind_speed: Some(11.7),
                wind_direction: Some(230),
                precipitation: Some(0.4),
            }),
            hours: Vec::new(),
            days: Vec::new(),
            alerts: Vec::new(),
        }
    }

    /// A fresh panel shows nothing rather than a placeholder, and the group hides itself. A `here`
    /// place with no fix and a place with no reading are the same absence.
    #[test]
    fn a_place_with_no_current_reading_renders_no_chip() {
        let mut applet = applet();
        applet.refresh();
        assert!(applet.indicators().is_empty(), "no value yet is no chip");

        applet.place = Some(PlaceWeather {
            current: None,
            ..reading()
        });
        applet.refresh();
        assert!(
            applet.indicators().is_empty(),
            "a place the provider answered for with no current reading is the same absence"
        );

        applet.place = Some(reading());
        applet.refresh();
        assert_eq!(applet.indicators().len(), 1);
    }

    #[test]
    fn the_chip_reads_the_one_rounding_site_and_carries_a_symbolic_icon() {
        let mut applet = applet();
        applet.place = Some(reading());
        applet.refresh();

        let [chip] = applet.indicators().try_into().expect("one chip");
        assert_eq!(chip.label.as_deref(), Some("18°"));
        assert!(chip.icon.is_some());
        assert_eq!(
            chip.tooltip, None,
            "an applet with no tooltip-format shows no tooltip"
        );
    }

    /// The icon is a `gio::Icon` built per condition change, not per render: `indicators` is a
    /// pull the runtime makes after every input.
    #[test]
    fn the_icon_is_rebuilt_only_when_the_condition_changes() {
        let mut applet = applet();
        applet.place = Some(reading());
        applet.refresh();
        let first = applet.icon.clone().expect("an icon").1;

        applet.refresh();
        let again = applet.icon.clone().expect("an icon").1;
        assert!(
            first.equal(Some(&again)),
            "an unchanged condition reuses the icon it already built"
        );

        let mut cleared = reading();
        if let Some(current) = cleared.current.as_mut() {
            current.condition = Condition::ClearSky;
        }
        applet.place = Some(cleared);
        applet.refresh();
        let changed = applet.icon.clone().expect("an icon").1;
        assert!(!first.equal(Some(&changed)));
    }

    /// A warning standing is what the bar is for. It takes the icon and the colour, and the
    /// temperature stays, so the chip does not stop being a weather chip.
    #[test]
    fn an_alert_takes_the_chips_icon_and_its_color() {
        let mut applet = applet();
        applet.place = Some(reading());
        applet.refresh();

        let [calm] = applet.indicators().try_into().expect("one chip");
        assert_eq!(calm.severity, None);

        let mut warned = reading();
        warned.alerts = vec![WeatherAlert {
            severity: AlertSeverity::Severe,
            headline: "Thunderstorm warning until 21:00".to_owned(),
            description: None,
            source: None,
            starts_at: None,
            expires_at: None,
        }];
        applet.place = Some(warned);
        applet.refresh();

        let [raised] = applet.indicators().try_into().expect("one chip");
        assert_eq!(raised.severity, Some(Severity::Error));
        assert_eq!(
            applet.icon.as_ref().map(|(name, _)| name.as_str()),
            Some(render::ALERT_ICON)
        );
        assert_eq!(
            raised.label.as_deref(),
            Some("18°"),
            "the reading stays: an alert changes what the chip warns about, not what it reads"
        );
    }

    #[test]
    fn a_configured_label_wins_over_the_coordinates_the_provider_answered_with() {
        let mut applet = applet();
        applet.place = Some(reading());
        assert_eq!(applet.label(), "Vilnius, LT");

        applet.settings.label = Some("Vilnius".to_owned());
        assert_eq!(applet.label(), "Vilnius");
    }

    #[test]
    fn a_missing_canonical_name_falls_back_to_coordinates() {
        let mut applet = applet();
        applet.place = Some(PlaceWeather {
            city: None,
            country_code: None,
            ..reading()
        });
        assert_eq!(applet.label(), "54.69, 25.28");
    }

    #[test]
    fn a_place_table_becomes_the_place_the_wire_takes() {
        assert_eq!(watched(&WeatherPlace::Here {}), WatchedPlace::Here);
        assert_eq!(
            watched(&WeatherPlace::Coordinates {
                latitude: 54.6872,
                longitude: 25.2797
            }),
            WatchedPlace::Coordinates {
                latitude: 54.6872,
                longitude: 25.2797
            }
        );
        assert_eq!(
            watched(&WeatherPlace::Location {
                name: "Vilnius, LT".to_owned(),
            }),
            WatchedPlace::Location {
                name: "Vilnius, LT".to_owned(),
            }
        );
    }
}
