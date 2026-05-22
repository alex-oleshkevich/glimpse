use std::collections::HashSet;

use chrono::{Local, NaiveDate};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::{
    components::{animated_popover::AnimatedPopover, popover_shell::PopoverShell},
    services::{
        calendar_events::{CalendarDaySnapshot, MonthKey, State as CalendarState},
        clock::State as ClockState,
    },
    widgets::{calendar::Calendar, date_hero::DateHero, events::Events, world_clock::WorldClock},
};

use super::format;

pub struct Popover {
    animation: AnimatedPopover,
    selected_date: NaiveDate,
    visible_month: MonthKey,
    clock: ClockState,
    calendar: CalendarState,
    hide_all_day_events: bool,
    date: DateHero,
    calendar_view: Calendar,
    world_clock: WorldClock,
    events: Events,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
    pub clock: ClockState,
    pub calendar: CalendarState,
    pub hide_all_day_events: bool,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    UpdateClock(ClockState),
    UpdateCalendar(CalendarState),
    CalendarSelectedDate(NaiveDate),
    CalendarMonthChanged(MonthKey),
    SetHideAllDayEvents(bool),
}

#[derive(Debug, Clone)]
pub enum PopoverOutput {
    VisibleMonthChanged(MonthKey),
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = PopoverOutput;

    view! {
        root = gtk::Popover {
            add_css_class: "clock-popover",
            add_css_class: "popover-size-xlarge",
            set_hexpand: false,

            #[template]
            PopoverShell {
                #[template_child]
                footer {
                    set_visible: false,
                },

                #[template_child]
                content {
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 0,
                        add_css_class: "clock-popover-layout",

                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            add_css_class: "clock-popover-left",

                            #[local_ref]
                            date_widget -> gtk::Box {},

                            #[local_ref]
                            calendar_widget -> gtk::Box {},

                            #[local_ref]
                            world_clock_widget -> gtk::Box {},
                        },

                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            add_css_class: "clock-popover-right",

                            #[local_ref]
                            events_widget -> gtk::Box {},
                        },
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let selected_date = Local::now().date_naive();
        let visible_month = MonthKey::from_date(selected_date);
        let date = DateHero::new();
        let calendar_view = Calendar::new();
        calendar_view.set_selected_date(selected_date);
        {
            let input = sender.input_sender().clone();
            calendar_view.connect_day_selected(move |cal| {
                let _ = input.send(PopoverInput::CalendarSelectedDate(cal.selected_date()));
            });
        }
        {
            let input = sender.input_sender().clone();
            calendar_view.connect_month_changed(move |cal| {
                let _ = input.send(PopoverInput::CalendarMonthChanged(cal.visible_month()));
            });
        }
        let world_clock = WorldClock::new();
        world_clock.set_rows(&init.clock.world);
        let events = Events::new();

        let date_widget: gtk::Box = date.clone().upcast();
        let calendar_widget: gtk::Box = calendar_view.clone().upcast();
        let world_clock_widget: gtk::Box = world_clock.clone().upcast();
        let events_widget: gtk::Box = events.clone().upcast();

        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);

        let mut model = Popover {
            animation: AnimatedPopover::new(&widgets.root),
            selected_date,
            visible_month,
            clock: init.clock,
            calendar: init.calendar,
            hide_all_day_events: init.hide_all_day_events,
            date,
            calendar_view,
            world_clock,
            events,
        };
        model.sync_all();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => self.animation.toggle(),
            PopoverInput::UpdateClock(clock) => {
                self.clock = clock;
                self.world_clock.set_rows(&self.clock.world);
                self.events.tick();
            }
            PopoverInput::UpdateCalendar(calendar) => {
                self.calendar = calendar;
                self.sync_calendar_state();
            }
            PopoverInput::CalendarSelectedDate(date) => {
                self.selected_date = date;
                self.sync_selected_date();
            }
            PopoverInput::CalendarMonthChanged(month) => {
                self.visible_month = month;
                self.sync_calendar_state();
                let _ = sender.output(PopoverOutput::VisibleMonthChanged(month));
            }
            PopoverInput::SetHideAllDayEvents(hide) => {
                if self.hide_all_day_events != hide {
                    self.hide_all_day_events = hide;
                    self.sync_calendar_state();
                }
            }
        }
    }
}

impl Popover {
    fn sync_all(&mut self) {
        self.sync_selected_date();
        self.sync_calendar_state();
        self.world_clock.set_rows(&self.clock.world);
    }

    fn sync_selected_date(&mut self) {
        let today = Local::now().date_naive();
        let weekday = if self.selected_date == today {
            "Today".into()
        } else {
            format::selected_weekday(self.selected_date)
        };
        self.date.set_weekday(&weekday);
        self.date
            .set_date(&format::selected_date(self.selected_date));
        self.calendar_view.set_selected_date(self.selected_date);
        self.sync_events();
    }

    fn sync_calendar_state(&mut self) {
        let hide_all_day = self.hide_all_day_events;
        let event_days: HashSet<NaiveDate> = self
            .calendar
            .month_cache
            .get(&self.visible_month)
            .map(|m| {
                m.day_snapshots
                    .values()
                    .filter(|d| d.events.iter().any(|e| !hide_all_day || !e.all_day))
                    .filter_map(|d| d.date.to_naive_date())
                    .collect()
            })
            .unwrap_or_default();
        self.calendar_view.set_event_days(&event_days);
        self.sync_events();
    }

    fn sync_events(&mut self) {
        let day = selected_day(&self.calendar, self.selected_date);
        let loading = self
            .calendar
            .loading_months
            .contains(&MonthKey::from_date(self.selected_date))
            && day.is_none();
        let mut events = day.map(|d| d.events).unwrap_or_default();
        if self.hide_all_day_events {
            events.retain(|e| !e.all_day);
        }
        self.events.set_data(self.selected_date, &events, loading);
    }
}

fn selected_day(state: &CalendarState, date: NaiveDate) -> Option<CalendarDaySnapshot> {
    let key = MonthKey::from_date(date);
    let calendar_date =
        glimpse_core::services::calendar_events::CalendarDate::from_naive_date(date);
    state
        .month_cache
        .get(&key)
        .and_then(|month| month.day_snapshots.get(&calendar_date))
        .cloned()
        .or_else(|| {
            state.month_cache.get(&key).map(|_| CalendarDaySnapshot {
                date: calendar_date,
                events: Vec::new(),
            })
        })
}
