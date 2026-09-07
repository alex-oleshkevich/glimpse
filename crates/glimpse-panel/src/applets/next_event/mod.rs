mod render;

use std::time::Duration;

use chrono::{DateTime, Local, TimeDelta};
use glimpse_config::{Applet as AppletConfig, AppletKind, NextEventConfig};
use glimpse_contracts::{CalendarEvents, Message as _};
use glimpse_widgets::{IndicatorSpec, NextEventPopover};
use gtk4::glib;

use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{Applet, Ctx, Input, payload};
use crate::applets::agenda::{self, Occasion};

const MINUTE: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct NextEvent {
    settings: NextEventConfig,
    events: Vec<Occasion>,
    twelve: bool,
    tooltip_format: Option<String>,
    footer: Option<(String, Vec<String>)>,
    chosen: Option<usize>,
    spec: Vec<IndicatorSpec>,
    shown: glib::WeakRef<NextEventPopover>,
}

impl Applet for NextEvent {
    fn topics(&self) -> &'static [&'static str] {
        &[CalendarEvents::NAME]
    }

    fn start() -> Self {
        Self {
            twelve: agenda::locale_is_twelve_hour(),
            ..Default::default()
        }
    }

    fn configure(&mut self, ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::NextEvent(settings) = &config.kind else {
            return;
        };
        self.settings = settings.clone();
        self.tooltip_format = config.common.tooltip_format.clone();
        self.footer = config
            .common
            .settings()
            .map(|(label, command)| (label.to_owned(), command.to_vec()));

        ctx.interval(MINUTE);
        self.refresh();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        match input {
            Input::Topic(event) => {
                let Some(events) = payload::<CalendarEvents>(event) else {
                    return;
                };
                self.events = agenda::occasions(&events.events);
            }
            Input::Tick | Input::Woken => {}
            _ => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, _seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = NextEventPopover::new();

        if let Some((_, command)) = &self.footer {
            let command = command.clone();
            shown.connect_footer_activated(move |_| run(&command));
        }

        self.shown.set(Some(&shown));
        self.refresh();
        Some(Box::new(shown))
    }
}

impl NextEvent {
    fn within(&self) -> TimeDelta {
        render::window(self.settings.within)
    }

    fn counting(&self) -> TimeDelta {
        render::window(self.settings.countdown)
    }

    fn horizon(&self) -> TimeDelta {
        render::window(self.settings.horizon.max(self.settings.within))
    }

    fn clock(&self) -> &'static str {
        match self.twelve {
            true => agenda::TWELVE,
            false => agenda::TWENTY_FOUR,
        }
    }

    fn refresh(&mut self) {
        let now = Local::now();
        self.chosen = render::next(now, &self.events, self.within(), self.settings.all_day);
        self.spec = self.indicator(now).into_iter().collect();

        if let Some(shown) = self.shown.upgrade() {
            self.dress(now, &shown);
        }
    }

    fn indicator(&self, now: DateTime<Local>) -> Option<IndicatorSpec> {
        let event = self.events.get(self.chosen?)?;
        Some(IndicatorSpec {
            dot: event.color,
            label: Some(render::label(now, event, self.counting())),
            tooltip: self.tooltip_format.as_deref().map(|format| {
                render::tooltip(format, event, &render::reading(now, event, self.clock()))
            }),
            ..Default::default()
        })
    }

    fn dress(&self, now: DateTime<Local>, shown: &NextEventPopover) {
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

        let Some(event) = self.chosen.and_then(|index| self.events.get(index)) else {
            shown.set_nothing();
            return;
        };

        let (title, subtitle) = render::heading(now, event, clock);
        let countdown = render::countdown(now, event);

        shown.set_heading(&title, Some(subtitle.as_str()));
        shown.set_countdown(countdown.as_ref().map(render::Countdown::readout));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bar showing an event the list below it does not contain is an incoherent popover, and
    /// nothing stops a document asking for one.
    #[test]
    fn the_list_never_reaches_less_far_than_the_bar() {
        let applet = NextEvent {
            settings: NextEventConfig {
                within: 180,
                horizon: 60,
                ..NextEventConfig::default()
            },
            ..NextEvent::default()
        };

        assert_eq!(applet.horizon(), applet.within());
        assert_eq!(applet.horizon(), TimeDelta::minutes(180));
    }
}
