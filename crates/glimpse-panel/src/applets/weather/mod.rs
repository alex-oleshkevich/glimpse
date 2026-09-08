mod render;

use std::time::Duration;

use chrono::Utc;
use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind, WeatherAppletConfig, WeatherPlace};
use glimpse_contracts::{
    Condition, GeoCoordinates, Message as _, PlaceWeather, UnitSystem, WatchedPlace, WeatherStatus,
    WeatherWatch,
};
use glimpse_widgets::{IndicatorSpec, WeatherPopover};
use gtk4::{gio, glib, prelude::*};

use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{Applet, Ctx, Input, payload};
use crate::applets::agenda;

/// The daemon holds a `weather.watch` for thirty minutes, so a minute's tick renews it with thirty
/// ticks of margin. If this period ever grows, `LEASE` in the weather service moves with it.
const MINUTE: Duration = Duration::from_secs(60);

pub struct Weather {
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
}

impl Applet for Weather {
    fn topics(&self) -> &'static [&'static str] {
        &[WeatherStatus::NAME]
    }

    fn start() -> Self {
        Self {
            settings: WeatherAppletConfig::default(),
            watching: WatchedPlace::Here,
            units: UnitSystem::Metric,
            place: None,
            twelve: agenda::locale_is_twelve_hour(),
            tooltip_format: None,
            footer: None,
            icon: None,
            spec: Vec::new(),
            shown: glib::WeakRef::new(),
        }
    }

    fn configure(&mut self, ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Weather(settings) = &config.kind else {
            return;
        };
        let watching = watched(&settings.place);
        if watching != self.watching {
            self.place = None;
        }
        self.watching = watching;
        self.settings = settings.clone();
        self.tooltip_format = config.common.tooltip_format.clone();
        self.footer = config
            .common
            .settings()
            .map(|(label, command)| (label.to_owned(), command.to_vec()));

        ctx.interval(MINUTE);
        self.renew(ctx);
        self.refresh();
    }

    fn handle(&mut self, ctx: &Ctx, input: &Input) {
        match input {
            Input::Topic(event) => {
                let Some(status) = payload::<WeatherStatus>(event) else {
                    return;
                };
                self.units = status.units;
                self.place = status
                    .places
                    .into_iter()
                    .find(|place| place.place == self.watching);
            }
            Input::Tick => self.renew(ctx),
            Input::Woken => {}
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
    fn renew(&self, ctx: &Ctx) {
        ctx.call::<WeatherWatch>(WeatherWatch {
            place: self.watching.clone(),
        });
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
        let place = self.place.as_ref()?;
        let current = place.current.clone()?;
        let severity = render::worst(&place.alerts);
        let name = match severity.is_some() {
            true => render::ALERT_ICON,
            false => render::icon(current.condition, current.is_day),
        };
        let icon = self.themed(name);
        let label = self.label();

        Some(IndicatorSpec {
            icon: Some(icon),
            label: Some(render::reading(current.temperature)),
            tooltip: self
                .tooltip_format
                .as_deref()
                .map(|format| render::tooltip(format, &label, &current)),
            severity,
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
        match self.place.as_ref().map(|place| &place.coordinates) {
            Some(coordinates) => pair(coordinates),
            None => gettext("Here"),
        }
    }

    fn dress(&self, shown: &WeatherPopover) {
        shown.set_footer(self.footer.as_ref().map(|(label, _)| label.as_str()));

        let Some(place) = self.place.as_ref() else {
            shown.set_heading(
                render::icon(Condition::Unknown, true),
                &self.label(),
                Some(&gettext("No reading yet")),
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
                    render::subtitle(current).as_deref(),
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
                    Some(&gettext("No reading yet")),
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
    }
}

fn pair(coordinates: &GeoCoordinates) -> String {
    format!("{:.2}, {:.2}", coordinates.latitude, coordinates.longitude)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;
    use glimpse_contracts::{AlertSeverity, CurrentWeather, GeoCoordinates, WeatherAlert};
    use glimpse_widgets::Severity;

    use super::*;

    fn reading() -> PlaceWeather {
        PlaceWeather {
            place: WatchedPlace::Here,
            coordinates: GeoCoordinates {
                latitude: 54.6872,
                longitude: 25.2797,
            },
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
        let mut applet = Weather::start();
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
        let mut applet = Weather::start();
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
        let mut applet = Weather::start();
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
        let mut applet = Weather::start();
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
    fn a_configured_label_wins_over_the_coordinates_the_daemon_answered_with() {
        let mut applet = Weather::start();
        applet.place = Some(reading());
        assert_eq!(applet.label(), "54.69, 25.28");

        applet.settings.label = Some("Vilnius".to_owned());
        assert_eq!(applet.label(), "Vilnius");
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
    }
}
