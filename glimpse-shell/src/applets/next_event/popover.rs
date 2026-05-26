use chrono::{DateTime, Local};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::widgets::{
    animated_popover::AnimatedPopover,
    empty_state::EmptyState,
    hero::Hero,
    key_value_grid::KeyValueGrid,
    popover_shell::PopoverShell,
    status_dot::{StatusDot, StatusDotStatus},
    tile::Tile,
};

use super::format::{self, NextEvent};

pub struct Popover {
    popover: AnimatedPopover,
    color_dot: StatusDot,
    details: KeyValueGrid,
    event: Option<NextEvent>,
    now: DateTime<Local>,
    title: String,
    subtitle: String,
    description: String,
    has_event: bool,
    has_details: bool,
    description_visible: bool,
    join_visible: bool,
    open_visible: bool,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    Update {
        event: Option<NextEvent>,
        now: DateTime<Local>,
    },
    Join,
    Open,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopoverOutput {
    Opened,
    Closed,
    OpenUri(String),
}

#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = PopoverOutput;

    view! {
        root = AnimatedPopover {
            add_css_class: "popover-size-medium",
            connect_show[sender] => move |_| {
                let _ = sender.output(PopoverOutput::Opened);
            },
            connect_closed[sender] => move |_| {
                let _ = sender.output(PopoverOutput::Closed);
            },

            PopoverShell {
                #[local_ref]
                hero_widget -> Hero {
                    #[watch]
                    set_visible: model.has_event,
                    #[watch]
                    set_title: &model.title,
                    #[watch]
                    set_subtitle: &model.subtitle,
                },

                #[local_ref]
                empty_widget -> EmptyState {
                    #[watch]
                    set_visible: !model.has_event,
                },

                #[local_ref]
                details_widget -> KeyValueGrid {
                    #[watch]
                    set_visible: model.has_details,
                },

                #[local_ref]
                description_widget -> Tile {
                    #[watch]
                    set_visible: model.description_visible,
                    set_primary: "Description",
                    #[watch]
                    set_secondary: Some(&model.description),
                },

                #[local_ref]
                join_widget -> Tile {
                    #[watch]
                    set_visible: model.join_visible,
                    set_primary: "Join meeting",
                    connect_activated[sender] => move |_| {
                        sender.input(PopoverInput::Join);
                    },
                },

                #[local_ref]
                open_widget -> Tile {
                    #[watch]
                    set_visible: model.open_visible,
                    set_primary: "Open event",
                    connect_activated[sender] => move |_| {
                        sender.input(PopoverInput::Open);
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let hero = Hero::new();
        hero.set_icon(Some("x-office-calendar-symbolic"));
        hero.set_icon_size(32);
        hero.set_trailing_visible(true);
        hero.set_toggle_visible(false);

        let color_dot = StatusDot::new();
        color_dot.set_valign(gtk::Align::Center);
        hero.append_trailing(&color_dot);

        let empty = EmptyState::new();
        empty.set_title("No upcoming event");
        empty.set_subtitle(Some(
            "The next event applet will show meetings inside its configured threshold.",
        ));

        let details = KeyValueGrid::new();
        let description = Tile::new();
        let join = Tile::new();
        join.set_left(Some(gtk::Image::from_icon_name("camera-video-symbolic")));
        let open = Tile::new();
        open.set_left(Some(gtk::Image::from_icon_name(
            "x-office-calendar-symbolic",
        )));

        let hero_widget = &hero;
        let empty_widget = &empty;
        let details_widget = &details;
        let description_widget = &description;
        let join_widget = &join;
        let open_widget = &open;

        let model = Popover::empty(root.clone(), color_dot, details.clone());
        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => self.popover.toggle(),
            PopoverInput::Update { event, now } => self.apply_event(event, now),
            PopoverInput::Join => {
                if let Some(uri) = action_state(self.event.as_ref()).join_uri {
                    let _ = sender.output(PopoverOutput::OpenUri(uri));
                }
            }
            PopoverInput::Open => {
                if let Some(uri) = action_state(self.event.as_ref()).open_uri {
                    let _ = sender.output(PopoverOutput::OpenUri(uri));
                }
            }
        }
    }
}

impl Popover {
    fn empty(popover: AnimatedPopover, color_dot: StatusDot, details: KeyValueGrid) -> Self {
        Self {
            popover,
            color_dot,
            details,
            event: None,
            now: Local::now(),
            title: String::new(),
            subtitle: String::new(),
            description: String::new(),
            has_event: false,
            has_details: false,
            description_visible: false,
            join_visible: false,
            open_visible: false,
        }
    }

    fn apply_event(&mut self, event: Option<NextEvent>, now: DateTime<Local>) {
        self.now = now;
        self.event = event;
        let Some(event) = self.event.as_ref() else {
            self.has_event = false;
            self.has_details = false;
            self.description_visible = false;
            self.join_visible = false;
            self.open_visible = false;
            self.details.clear();
            return;
        };

        self.has_event = true;
        self.title = event.title.clone();
        self.subtitle = format!(
            "{} · {}",
            format::time_range(event, now),
            format::remaining_label(event, now)
        );
        self.description = format::description_preview(event).unwrap_or_default();
        self.description_visible = !self.description.is_empty();
        self.join_visible = event
            .meeting_url
            .as_deref()
            .is_some_and(|value| !value.is_empty());
        self.open_visible = event.url.as_deref().is_some_and(|value| !value.is_empty());

        if !self.color_dot.set_color(event.source.color.as_deref()) {
            self.color_dot.set_status(StatusDotStatus::Neutral);
        }

        self.details.clear();
        self.details.add_row("Calendar", &event.source.display_name);
        self.details
            .add_row("Duration", &format::duration_label(event));
        add_optional_row(&self.details, "Status", event.status_label());
        add_optional_row(&self.details, "Location", event.location.clone());
        add_optional_row(&self.details, "Organizer", format::organizer_label(event));
        add_optional_row(&self.details, "Attendees", format::attendee_summary(event));
        self.has_details = true;
    }
}

fn add_optional_row(grid: &KeyValueGrid, key: &str, value: Option<String>) {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return;
    };
    grid.add_row(key, &value);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ActionState {
    pub join_uri: Option<String>,
    pub open_uri: Option<String>,
}

pub(super) fn action_state(event: Option<&NextEvent>) -> ActionState {
    let Some(event) = event else {
        return ActionState {
            join_uri: None,
            open_uri: None,
        };
    };

    ActionState {
        join_uri: event.meeting_url.clone().filter(|value| !value.is_empty()),
        open_uri: event.url.clone().filter(|value| !value.is_empty()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use glimpse_core::services::calendar_events::model::CalendarSource;

    fn local(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(y, m, d, h, min, 0)
            .single()
            .expect("valid local time")
    }

    fn event() -> NextEvent {
        NextEvent {
            title: "Planning".into(),
            start: local(2026, 5, 22, 10, 15),
            end: local(2026, 5, 22, 11, 0),
            source: CalendarSource {
                source_id: "work".into(),
                display_name: "Work".into(),
                color: Some("#4285f4".into()),
            },
            location: Some("Room A".into()),
            description: Some("Discuss Q2 scope".into()),
            url: Some("https://calendar.example/event".into()),
            meeting_url: Some("https://zoom.us/j/123".into()),
            status: Some("CONFIRMED".into()),
            organizer: None,
            attendees: Vec::new(),
            transparency: None,
            last_modified: None,
            sequence: None,
        }
    }

    #[test]
    fn action_state_is_empty_without_event() {
        let state = action_state(None);

        assert_eq!(
            state,
            ActionState {
                join_uri: None,
                open_uri: None,
            }
        );
    }

    #[test]
    fn action_state_exposes_link_actions_only() {
        let event = event();
        let state = action_state(Some(&event));

        assert_eq!(state.join_uri.as_deref(), Some("https://zoom.us/j/123"));
        assert_eq!(
            state.open_uri.as_deref(),
            Some("https://calendar.example/event")
        );
    }
}
