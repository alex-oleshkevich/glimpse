pub(crate) mod render;

use std::time::Duration;

use chrono::Utc;
use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind, WeatherAppletConfig, WeatherPlace};
use glimpse_dbus::weather::{Condition, GeoCoordinates, PlaceWeather, UnitSystem, WatchedPlace};
use glimpse_dbus::weather::{WeatherProviderHandle, WeatherProviderState};
use glimpse_widgets::{IndicatorSpec, Severity, WeatherPopover};
use gtk4::{gio, glib, prelude::*};

use crate::applet::popover::Seat;
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
        self.tooltip_format = config.kind.tooltip_format().map(str::to_owned);

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

    fn popover(&mut self, _seat: &Seat) -> Option<gtk4::Widget> {
        let shown = WeatherPopover::new();

        self.shown.set(Some(&shown));
        self.refresh();
        Some(shown.upcast())
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
            false => current.condition.icon_name(current.is_day),
        };
        let icon = self.themed(name);
        let label = self.label();

        Some(IndicatorSpec {
            icon: Some(icon),
            label: Some(glimpse_dbus::weather::reading(current.temperature)),
            tooltip: self.trouble.clone().or_else(|| {
                let format = self
                    .tooltip_format
                    .clone()
                    .unwrap_or_else(render::default_tooltip);
                Some(render::tooltip(&format, &label, &current))
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
            icon: Some(self.themed(Condition::Unknown.icon_name(true))),
            tooltip: Some(reason),
            severity: Some(Severity::Warning),
            ..Default::default()
        })
    }

    fn themed(&mut self, name: &str) -> gio::Icon {
        match &self.icon {
            Some((held, icon)) if held == name => icon.clone(),
            _ => {
                let icon: gio::Icon = gio::ThemedIcon::new(name).upcast();
                self.icon = Some((name.to_owned(), icon.clone()));
                icon
            }
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
        let Some(place) = self.place.as_ref() else {
            shown.set_heading(
                Condition::Unknown.icon_name(true),
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
                    current.condition.icon_name(current.is_day),
                    &self.label(),
                    self.trouble
                        .as_deref()
                        .or(render::subtitle(current).as_deref()),
                );
                shown.set_reading(Some((
                    &glimpse_dbus::weather::rounded(current.temperature),
                    glimpse_dbus::weather::DEGREE,
                )));
            }
            None => {
                shown.set_heading(
                    Condition::Unknown.icon_name(true),
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
mod tests;
