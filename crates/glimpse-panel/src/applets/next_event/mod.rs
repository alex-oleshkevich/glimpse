mod render;

use std::time::Duration;

use chrono::{DateTime, Local, TimeDelta};
use glimpse_config::{Applet as AppletConfig, AppletKind, NextEventConfig};
use glimpse_services::CalendarHandle;
use glimpse_widgets::{IndicatorSpec, NextEventPopover};
use gtk4::{gio, glib, prelude::*};

use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{Applet, Ctx, Input, Opener};
use crate::applets::agenda::{self, Occasion};

const MINUTE: Duration = Duration::from_secs(60);

pub struct NextEvent {
    calendar: CalendarHandle,
    settings: NextEventConfig,
    events: Vec<Occasion>,
    twelve: bool,
    tooltip_format: Option<String>,
    footer: Option<(String, Vec<String>)>,
    chosen: Option<usize>,
    spec: Vec<IndicatorSpec>,
    shown: glib::WeakRef<NextEventPopover>,
    icon: gio::Icon,
}

impl Applet for NextEvent {
    fn configure(&mut self, ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::NextEvent(settings) = &config.kind else {
            return;
        };
        self.settings = settings.clone();
        self.twelve = config.regional.twelve_hour();
        self.tooltip_format = config.common.tooltip_format.clone();
        self.footer = config
            .common
            .settings()
            .map(|(label, command)| (label.to_owned(), command.to_vec()));

        ctx.interval(MINUTE);
        self.refresh(&ctx.opener());
    }

    fn handle(&mut self, ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => {
                self.events = agenda::occasions(&self.calendar.snapshot().events);
            }
            Input::Tick => {}
            _ => return,
        }
        self.refresh(&ctx.opener());
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = NextEventPopover::new();

        if let Some((_, command)) = &self.footer {
            let command = command.clone();
            shown.connect_footer_activated(move |_| run(&command));
        }
        shown.connect_join_activated(|_, url| {
            if url.starts_with("https://") || url.starts_with("http://") {
                run(&["xdg-open".to_owned(), url]);
            }
        });
        shown.connect_open_event_activated(|_, url| {
            if url.starts_with("https://") || url.starts_with("http://") {
                run(&["xdg-open".to_owned(), url]);
            }
        });

        self.shown.set(Some(&shown));
        self.refresh(&seat.opener());
        Some(Box::new(shown))
    }
}

impl NextEvent {
    pub fn start(calendar: CalendarHandle) -> Self {
        let events = calendar.snapshot();
        Self {
            calendar,
            settings: NextEventConfig::default(),
            events: agenda::occasions(&events.events),
            twelve: false,
            tooltip_format: None,
            footer: None,
            chosen: None,
            spec: Vec::new(),
            shown: glib::WeakRef::default(),
            icon: gio::ThemedIcon::new("appointment-soon-symbolic").upcast(),
        }
    }

    fn within(&self) -> TimeDelta {
        render::window(self.settings.within)
    }

    fn counting(&self) -> TimeDelta {
        render::window(self.settings.countdown)
    }

    fn horizon(&self) -> TimeDelta {
        horizon(&self.settings)
    }

    fn clock(&self) -> &'static str {
        glimpse_config::clock(self.twelve)
    }

    fn refresh(&mut self, opener: &Opener) {
        let now = Local::now();
        self.chosen = render::next(now, &self.events, self.within(), self.settings.all_day);
        self.spec = self.indicator(now).into_iter().collect();

        let Some(shown) = self.shown.upgrade() else {
            return;
        };
        match self.event() {
            Some(_) => self.dress(now, &shown),
            None => opener.close_popover(),
        }
    }

    fn indicator(&self, now: DateTime<Local>) -> Option<IndicatorSpec> {
        let event = self.event()?;
        Some(IndicatorSpec {
            icon: Some(self.icon.clone()),
            dot: event.color,
            label: Some(render::label(now, event, self.counting())),
            tooltip: self.tooltip_format.as_deref().map(|format| {
                let clock = self.clock();
                render::tooltip(
                    format,
                    event,
                    &render::reading(now, event, clock),
                    &render::conflicts(&self.events, self.chosen),
                )
            }),
            ..Default::default()
        })
    }

    fn event(&self) -> Option<&Occasion> {
        self.chosen.and_then(|index| self.events.get(index))
    }

    fn dress(&self, now: DateTime<Local>, shown: &NextEventPopover) {
        let Some(event) = self.event() else {
            return;
        };
        let clock = self.clock();
        shown.set_footer(self.footer.as_ref().map(|(label, _)| label.as_str()));
        shown.set_upcoming(&render::upcoming(
            now,
            &self.events,
            self.chosen,
            self.horizon(),
            self.settings.upcoming,
            clock,
        ));

        let (title, subtitle) = render::heading(now, event, clock);
        let countdown = render::countdown(now, event);
        let join = render::join(event);
        let open_event = render::open_event(event);

        shown.set_heading(&title, Some(subtitle.as_str()));
        shown.set_countdown(countdown.as_ref().map(render::Countdown::readout));
        shown.set_join(join.as_ref().map(|join| {
            (
                join.title.as_str(),
                join.subtitle.as_str(),
                join.url.as_str(),
            )
        }));
        shown.set_open_event(open_event.as_ref().map(|open| {
            (
                open.title.as_str(),
                open.subtitle.as_str(),
                open.url.as_str(),
            )
        }));
        shown.set_facts(&render::facts(
            event,
            &render::conflicts(&self.events, self.chosen),
        ));
    }
}

fn horizon(settings: &NextEventConfig) -> TimeDelta {
    render::window(settings.horizon.max(settings.within))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bar showing an event the list below it does not contain is an incoherent popover, and
    /// nothing stops a document asking for one.
    #[test]
    fn the_list_never_reaches_less_far_than_the_bar() {
        let settings = NextEventConfig {
            within: 180,
            horizon: 60,
            ..NextEventConfig::default()
        };

        assert_eq!(horizon(&settings), render::window(settings.within));
        assert_eq!(horizon(&settings), TimeDelta::minutes(180));
    }
}
