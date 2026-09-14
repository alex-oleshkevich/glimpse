use std::fmt::Write as _;
use std::time::Duration;

use chrono::{DateTime, Datelike as _, Local, TimeZone, Utc};
use chrono_tz::Tz;
use glimpse_config::{Applet as AppletConfig, AppletKind, ClockConfig};
use glimpse_services::CalendarHandle;
use glimpse_widgets::{CalendarPopover, IndicatorSpec};
use gtk4::glib;

use super::popover;
use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{Applet, Ctx, Input, spawn_command};
use crate::applets::agenda::{self, Occasion};

const SECOND: Duration = Duration::from_secs(1);
const MINUTE: Duration = Duration::from_secs(60);
const SUBMINUTE: [char; 8] = ['S', 'T', 'X', 'r', 'c', '+', 's', 'f'];

pub struct Clock {
    calendar: CalendarHandle,
    settings: ClockConfig,
    twelve: bool,
    tooltip_format: Option<String>,
    footer: Option<(String, Vec<String>)>,
    zone: Option<Tz>,
    events: Vec<Occasion>,
    truncated_from: Option<DateTime<Utc>>,
    range: Option<(DateTime<Utc>, DateTime<Utc>)>,
    shown: glib::WeakRef<CalendarPopover>,
}

impl Applet for Clock {
    fn configure(&mut self, ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Clock(clock) = &config.kind else {
            return;
        };
        self.settings = clock.clone();
        self.twelve = config.regional.twelve_hour();
        self.tooltip_format = config.common.tooltip_format.clone();
        self.footer = config
            .common
            .settings()
            .map(|(label, command)| (label.to_owned(), command.to_vec()));
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
            let events = self.calendar.snapshot();
            self.events = agenda::occasions(&events.events);
            self.truncated_from = events.truncated_from;
            if let Some(shown) = self.shown.upgrade() {
                self.dress(&shown);
            }
        }

        if !matches!(input, Input::Tick | Input::Woken) {
            return;
        }
        self.ask_for_range();
        if let Some(shown) = self.shown.upgrade() {
            self.paint(&shown);
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

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let today = Local::now().date_naive();

        let shown = CalendarPopover::new();
        shown.open_on(popover::ymd(today));
        self.range = None;

        let opener = seat.opener();
        shown.connect_day_selected(move |_, _| opener.wake());

        let opener = seat.opener();
        shown.connect_month_shown(move |_, _, _| opener.wake());

        if let Some((_, command)) = &self.footer {
            let command = command.clone();
            shown.connect_footer_activated(move |_| run(&command));
        }

        self.shown.set(Some(&shown));
        self.dress(&shown);
        Some(Box::new(shown))
    }
}

impl Clock {
    pub fn start(calendar: CalendarHandle) -> Self {
        let events = calendar.snapshot();
        Self {
            calendar,
            settings: ClockConfig::default(),
            twelve: false,
            tooltip_format: None,
            footer: None,
            zone: None,
            events: agenda::occasions(&events.events),
            truncated_from: events.truncated_from,
            range: None,
            shown: glib::WeakRef::default(),
        }
    }

    fn dress(&self, shown: &CalendarPopover) {
        shown.set_zones(&popover::zones(&self.settings.timezones));
        shown.set_twelve_hour(self.twelve);
        shown.set_markers(&popover::markers(&self.events));
        shown.set_footer(self.footer.as_ref().map(|(label, _)| label.as_str()));
        self.paint(shown);
    }

    fn paint(&self, shown: &CalendarPopover) {
        let now = Local::now();
        let day = shown
            .selected()
            .and_then(popover::date)
            .unwrap_or_else(|| now.date_naive());
        let clock = glimpse_config::clock(self.twelve);

        let (title, week) = popover::heading(day, self.settings.week_numbers);
        shown.set_heading(&title, week.as_deref());
        shown.set_day_truncated(popover::truncated(day, self.truncated_from));
        shown.set_day(
            &popover::day_title(day, now.date_naive()),
            &popover::rows(now, day, &self.events, clock),
        );
        if let Ok(instant) = glib::DateTime::from_unix_local(now.timestamp()) {
            shown.set_now(&instant);
        }
    }

    fn ask_for_range(&mut self) {
        let (year, month) = match self.shown.upgrade() {
            Some(shown) => shown.shown_month(),
            None => {
                let today = Local::now().date_naive();
                (today.year(), today.month())
            }
        };
        let Some(range) = popover::month_pair(year, month) else {
            return;
        };
        if self.range == Some(range) {
            return;
        }
        self.range = Some(range);
        let calendar = self.calendar.clone();
        spawn_command("calendar.set_range", async move {
            calendar.set_range(range.0, range.1).await
        });
    }

    fn read(&self, format: &str) -> Option<String> {
        match self.zone {
            Some(zone) => formatted(&Utc::now().with_timezone(&zone), format),
            None => formatted(&Local::now(), format),
        }
    }
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
mod tests {
    use super::{MINUTE, SECOND, formatted, period, ticks_below_a_minute};
    use chrono::{TimeZone, Utc};

    #[test]
    fn a_format_that_shows_seconds_asks_for_a_faster_tick() {
        assert_eq!(period("%H:%M:%S", None), SECOND);
        assert_eq!(period("%T", None), SECOND);
        assert_eq!(period("%H:%M", None), MINUTE);
    }

    #[test]
    fn a_padded_specifier_is_still_the_specifier_it_pads() {
        assert!(
            ticks_below_a_minute("%-S"),
            "a padding modifier hides the specifier from a plain substring search"
        );
        assert!(ticks_below_a_minute("%.3f"));
        assert!(!ticks_below_a_minute("%-d %b"));
    }

    #[test]
    fn an_escaped_percent_does_not_name_a_specifier() {
        assert!(
            !ticks_below_a_minute("100%%S"),
            "`%%` is a literal percent, so the S after it is text"
        );
        assert!(ticks_below_a_minute("100%% %S"));
    }

    #[test]
    fn the_tooltip_can_be_what_asks_for_the_faster_tick() {
        assert_eq!(period("%H:%M", Some("%H:%M:%S")), SECOND);
        assert_eq!(period("%H:%M", Some("%A")), MINUTE);
    }

    #[test]
    fn a_format_that_cannot_render_yields_nothing_rather_than_panicking() {
        let now = Utc.with_ymd_and_hms(2026, 9, 4, 15, 30, 0).unwrap();

        assert_eq!(formatted(&now, "%H:%M").as_deref(), Some("15:30"));
        assert!(
            formatted(&now, "%Q").is_none(),
            "chrono's Display returns an error for an unknown specifier, and to_string() turns \
             that into a panic that would stop the applet for good"
        );
    }
}
