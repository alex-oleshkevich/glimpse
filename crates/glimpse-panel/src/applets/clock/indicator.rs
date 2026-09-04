use std::fmt::Write as _;
use std::time::Duration;

use chrono::{DateTime, Local, TimeZone, Utc};
use chrono_tz::Tz;
use glimpse_config::{Applet as AppletConfig, AppletKind, ClockConfig, HourFormat};
use glimpse_widgets::{CalendarPopover, IndicatorSpec};
use gtk4::glib;

use super::agenda::Occasion;
use super::popover;
use crate::applet::popover::{PopoverHandle, Seat};
use crate::applet::{Applet, Ctx, Input};

const SECOND: Duration = Duration::from_secs(1);
const MINUTE: Duration = Duration::from_secs(60);
const SUBMINUTE: [char; 8] = ['S', 'T', 'X', 'r', 'c', '+', 's', 'f'];

#[derive(Default)]
pub struct Clock {
    settings: ClockConfig,
    twelve: bool,
    tooltip_format: Option<String>,
    footer: Option<(String, Vec<String>)>,
    zone: Option<Tz>,
    events: Vec<Occasion>,
    shown: glib::WeakRef<CalendarPopover>,
}

impl Applet for Clock {
    fn start() -> Self {
        Self::default()
    }

    fn configure(&mut self, ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Clock(clock) = &config.kind else {
            return;
        };
        self.settings = clock.clone();
        self.twelve = twelve_hour(clock.hour_format);
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

        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        let (Input::Tick | Input::Woken, Some(shown)) = (input, self.shown.upgrade()) else {
            return;
        };
        self.paint(&shown);
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
        self.events = popover::fixture(today);

        let shown = CalendarPopover::new();
        shown.open_on(popover::ymd(today));

        let opener = seat.opener();
        shown.connect_day_selected(move |_, _| opener.wake());

        if let Some((_, command)) = &self.footer {
            let command = command.clone();
            shown.connect_footer_activated(move |_| popover::run(&command));
        }

        self.shown.set(Some(&shown));
        self.dress(&shown);
        Some(Box::new(shown))
    }
}

impl Clock {
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
        popover::render(
            shown,
            now,
            day,
            &self.events,
            self.twelve,
            self.settings.week_numbers,
        );
    }

    fn read(&self, format: &str) -> Option<String> {
        match self.zone {
            Some(zone) => formatted(&Utc::now().with_timezone(&zone), format),
            None => formatted(&Local::now(), format),
        }
    }
}

fn twelve_hour(hour_format: HourFormat) -> bool {
    match hour_format {
        HourFormat::Twelve => true,
        HourFormat::TwentyFour => false,
        HourFormat::Locale => popover::locale_is_twelve_hour(),
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
