use std::fmt::Write as _;
use std::time::Duration;

use chrono::{DateTime, Local, TimeZone, Utc};
use chrono_tz::Tz;
use glimpse_config::{Applet as AppletConfig, AppletKind, ClockConfig};
use glimpse_dbus::calendar::CalendarProviderHandle;
use glimpse_services::CalendarEvents;
use glimpse_widgets::{CalendarPopover, IndicatorSpec};
use gtk4::glib;
use gtk4::prelude::Cast as _;
use tokio::sync::watch;

use super::popover;
use crate::applet::popover::Seat;
use crate::applet::{Applet, Ctx, Input};
use crate::applets::agenda::{self, Occasion};

const SECOND: Duration = Duration::from_secs(1);
const MINUTE: Duration = Duration::from_secs(60);
const SUBMINUTE: [char; 8] = ['S', 'T', 'X', 'r', 'c', '+', 's', 'f'];

#[derive(Debug, Clone, Default, PartialEq)]
struct Browsed {
    range: Option<(DateTime<Utc>, DateTime<Utc>)>,
    events: Vec<Occasion>,
    truncated_from: Option<DateTime<Utc>>,
    asked: Option<(DateTime<Utc>, DateTime<Utc>)>,
    generation: u64,
}

pub struct Clock {
    calendar: CalendarProviderHandle,
    settings: ClockConfig,
    twelve: bool,
    tooltip_format: Option<String>,
    zone: Option<Tz>,
    events: Vec<Occasion>,
    truncated_from: Option<DateTime<Utc>>,
    available: bool,
    revision: u64,
    range: Option<(DateTime<Utc>, DateTime<Utc>)>,
    query: u64,
    browsed: watch::Sender<Browsed>,
    pending: Option<watch::Receiver<Browsed>>,
    shown: glib::WeakRef<CalendarPopover>,
}

impl Applet for Clock {
    fn configure(&mut self, ctx: &Ctx, config: &AppletConfig) {
        if let Some(pending) = self.pending.take() {
            ctx.watch(pending);
        }
        let AppletKind::Clock(clock) = &config.kind else {
            return;
        };
        self.settings = clock.clone();
        self.twelve = config.regional.twelve_hour();
        self.tooltip_format = config.kind.tooltip_format().map(str::to_owned);

        self.zone = clock
            .timezone
            .as_deref()
            .and_then(|name| match name.parse() {
                Ok(zone) => Some(zone),
                Err(_) => {
                    tracing::warn!(
                        applet = ctx.name(),
                        timezone = name,
                        "unknown timezone, reading the local one instead"
                    );
                    None
                }
            });

        if self.read(&self.settings.label_format).is_none() {
            tracing::warn!(
                applet = ctx.name(),
                format = self.settings.label_format,
                "label-format has no rendering, so the clock shows nothing"
            );
        }

        ctx.interval(period(
            &self.settings.label_format,
            self.tooltip_format.as_deref(),
        ));

        self.ask_for_range();

        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        if matches!(input, Input::Woken) {
            let snapshot = self.calendar.snapshot();
            let events = agenda::occasions(&snapshot.events);
            let content = changed(
                (&self.events, &self.truncated_from, self.available),
                (&events, &snapshot.truncated_from, snapshot.available),
            );
            let drop_range = invalidate(
                (
                    &self.events,
                    &self.truncated_from,
                    self.available,
                    self.revision,
                ),
                (
                    &events,
                    &snapshot.truncated_from,
                    snapshot.available,
                    snapshot.revision,
                ),
                self.shown.upgrade().is_some(),
            );
            if content {
                self.events = events;
                self.truncated_from = snapshot.truncated_from;
                self.available = snapshot.available;
            }
            self.revision = snapshot.revision;
            if drop_range {
                self.range = None;
            }
        }

        if !matches!(input, Input::Tick | Input::Woken) {
            return;
        }
        if matches!(input, Input::Tick) && self.browsed.borrow().asked.is_none() {
            self.range = None;
        }
        self.ask_for_range();
        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        let Some(label) = self.read(&self.settings.label_format) else {
            return Vec::new();
        };
        vec![IndicatorSpec {
            label: Some(label),
            tooltip: self
                .tooltip_format
                .as_deref()
                .and_then(|format| self.read(format)),
            ..Default::default()
        }]
    }

    fn popover(&mut self, seat: &Seat) -> Option<gtk4::Widget> {
        let today = Local::now().date_naive();

        let shown = CalendarPopover::new();
        shown.open_on(popover::ymd(today));
        self.range = None;

        let opener = seat.opener();
        shown.connect_day_selected(move |_, _| opener.wake());

        let opener = seat.opener();
        shown.connect_month_shown(move |_, _, _| opener.wake());

        shown.connect_link_activated(|_, url| agenda::open_http(url));

        self.shown.set(Some(&shown));
        self.dress(&shown);
        Some(shown.upcast())
    }
}

impl Clock {
    pub fn start(calendar: CalendarProviderHandle) -> Self {
        let events = calendar.snapshot();
        let (browsed, pending) = watch::channel(Browsed::default());
        Self {
            calendar,
            settings: ClockConfig::default(),
            twelve: false,
            tooltip_format: None,
            zone: None,
            events: agenda::occasions(&events.events),
            truncated_from: events.truncated_from,
            available: events.available,
            revision: events.revision,
            range: None,
            query: 0,
            browsed,
            pending: Some(pending),
            shown: glib::WeakRef::default(),
        }
    }

    fn dress(&self, shown: &CalendarPopover) {
        let browsed = self.browsed.borrow();
        let (events, _) = painted(&browsed, self.range, &self.events, self.truncated_from);
        shown.set_zones(&popover::zones(&self.settings.timezones));
        shown.set_twelve_hour(self.twelve);
        shown.set_markers(&popover::markers(events, self.settings.hide_all_day));

        drop(browsed);
        self.paint(shown);
    }

    fn paint(&self, shown: &CalendarPopover) {
        let now = Local::now();
        let day = shown
            .selected()
            .and_then(popover::date)
            .unwrap_or_else(|| now.date_naive());
        let clock = glimpse_config::clock(self.twelve);
        let browsed = self.browsed.borrow();
        let (events, truncated_from) =
            painted(&browsed, self.range, &self.events, self.truncated_from);

        let (title, week) = popover::heading(now.date_naive(), self.settings.week_numbers);
        shown.set_heading(&title, week.as_deref());
        shown.set_day_truncated(popover::truncated(day, truncated_from));
        shown.set_day(
            &popover::day_title(day, now.date_naive()),
            &popover::rows(now, day, events, clock, self.settings.hide_all_day),
        );
        if let Ok(instant) = glib::DateTime::from_unix_local(now.timestamp()) {
            shown.set_now(&instant);
        }
    }

    fn ask_for_range(&mut self) {
        let Some(shown) = self.shown.upgrade() else {
            self.range = None;
            if *self.browsed.borrow() != Browsed::default() {
                let _ = self.browsed.send(Browsed::default());
            }
            return;
        };
        let (year, month) = shown.shown_month();
        let Some(range) = popover::month_pair(year, month) else {
            return;
        };
        if self.range == Some(range) {
            return;
        }
        self.range = Some(range);
        self.query = self.query.wrapping_add(1);
        let query = self.query;
        self.browsed.send_modify(|browsed| {
            browsed.asked = Some(range);
            browsed.generation = query;
        });
        let calendar = self.calendar.clone();
        let browsed = self.browsed.clone();
        relm4::spawn(async move {
            match calendar.events(range.0, range.1).await {
                Ok(events) => {
                    let _ =
                        browsed.send_if_modified(|browsed| accept(browsed, range, query, events));
                }
                Err(error) => {
                    let _ = browsed.send_if_modified(|browsed| failed(browsed, query));
                    tracing::warn!(%error, "reading calendar events failed");
                }
            }
        });
    }

    fn read(&self, format: &str) -> Option<String> {
        match self.zone {
            Some(zone) => formatted(&Utc::now().with_timezone(&zone), format),
            None => formatted(&Local::now(), format),
        }
    }
}

fn changed(
    previous: (&[Occasion], &Option<DateTime<Utc>>, bool),
    next: (&[Occasion], &Option<DateTime<Utc>>, bool),
) -> bool {
    previous != next
}

fn invalidate(
    previous: (&[Occasion], &Option<DateTime<Utc>>, bool, u64),
    next: (&[Occasion], &Option<DateTime<Utc>>, bool, u64),
    shown: bool,
) -> bool {
    shown && previous != next
}

fn painted<'a>(
    browsed: &'a Browsed,
    range: Option<(DateTime<Utc>, DateTime<Utc>)>,
    events: &'a [Occasion],
    truncated_from: Option<DateTime<Utc>>,
) -> (&'a [Occasion], Option<DateTime<Utc>>) {
    if browsed.range.is_some() && browsed.range == range {
        (&browsed.events, browsed.truncated_from)
    } else {
        (events, truncated_from)
    }
}

fn accept(
    browsed: &mut Browsed,
    range: (DateTime<Utc>, DateTime<Utc>),
    generation: u64,
    events: CalendarEvents,
) -> bool {
    if browsed.generation != generation || browsed.asked != Some(range) {
        return false;
    }
    browsed.range = Some(range);
    browsed.events = agenda::occasions(&events.events);
    browsed.truncated_from = events.truncated_from;
    true
}

fn failed(browsed: &mut Browsed, generation: u64) -> bool {
    if browsed.generation != generation {
        return false;
    }
    browsed.asked = None;
    true
}

fn formatted<Z: TimeZone>(now: &DateTime<Z>, format: &str) -> Option<String>
where
    Z::Offset: std::fmt::Display,
{
    let mut rendered = String::new();
    write!(rendered, "{}", now.format(format)).ok()?;
    Some(rendered)
}

fn period(label: &str, tooltip: Option<&str>) -> Duration {
    match ticks_below_a_minute(label) || tooltip.is_some_and(ticks_below_a_minute) {
        true => SECOND,
        false => MINUTE,
    }
}

fn ticks_below_a_minute(format: &str) -> bool {
    let mut characters = format.chars();
    while let Some(character) = characters.next() {
        if character != '%' {
            continue;
        }
        let mut next = characters.next();
        while next.is_some_and(|pad| pad.is_ascii_digit() || matches!(pad, '-' | '_' | '.')) {
            next = characters.next();
        }
        if next.is_some_and(|specifier| SUBMINUTE.contains(&specifier)) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests;
