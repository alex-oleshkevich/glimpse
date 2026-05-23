use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::{
    services::session::{
        SessionAction, SessionActionAvailability, SessionBackendState, SessionServiceHealth, State,
    },
    widgets::{
        animated_popover::AnimatedPopover, header::Header, hero::Hero, popover_shell::PopoverShell,
        tile::Tile,
    },
};

use super::{Config, format};

pub struct Popover {
    popover: AnimatedPopover,
    config: Config,
    state: State,
    hero_icon_name: &'static str,
    hero_subtitle: String,
    sections: ActionSections,
}

pub struct Init {
    pub parent: gtk::Box,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    Toggle,
    Close,
    UpdateState(State),
    Reconfigure(Config),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    ActionRequested(SessionAction),
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = Init;
    type Input = Input;
    type Output = Output;

    view! {
        root = AnimatedPopover {
            add_css_class: "popover-size-small",

            PopoverShell {

                Hero {
                    #[watch]
                    set_icon: Some(model.hero_icon_name),
                    #[watch]
                    set_title: &model.state.snapshot.user_name,
                    #[watch]
                    set_subtitle: &model.hero_subtitle,
                },

                #[name = "session_header"]
                Header {
                    set_label: "Session",
                    #[watch]
                    set_visible: model.sections.session_visible,
                },

                #[name = "lock_tile"]
                Tile {
                    set_primary: "Lock Screen",
                    #[watch]
                    set_visible: model.sections.action_visible(SessionAction::Lock),
                    #[wrap(Some)]
                    set_left = gtk::Image {
                        set_icon_name: Some("system-lock-screen-symbolic"),
                        set_pixel_size: 16,
                    },
                    connect_activated[sender] => move |_| {
                        let _ = sender.output(Output::ActionRequested(SessionAction::Lock));
                    },
                },

                #[name = "logout_tile"]
                Tile {
                    set_primary: "Log Out",
                    #[watch]
                    set_visible: model.sections.action_visible(SessionAction::Logout),
                    #[wrap(Some)]
                    set_left = gtk::Image {
                        set_icon_name: Some("system-log-out-symbolic"),
                        set_pixel_size: 16,
                    },
                    connect_activated[sender] => move |_| {
                        let _ = sender.output(Output::ActionRequested(SessionAction::Logout));
                    },
                },

                #[name = "action_separator"]
                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                    #[watch]
                    set_visible: model.sections.session_visible && model.sections.power_visible,
                },

                #[name = "power_header"]
                Header {
                    set_label: "Power",
                    #[watch]
                    set_visible: model.sections.power_visible,
                },

                #[name = "suspend_tile"]
                Tile {
                    set_primary: "Suspend",
                    #[watch]
                    set_visible: model.sections.action_visible(SessionAction::Suspend),
                    #[wrap(Some)]
                    set_left = gtk::Image {
                        set_icon_name: Some("media-playback-pause-symbolic"),
                        set_pixel_size: 16,
                    },
                    connect_activated[sender] => move |_| {
                        let _ = sender.output(Output::ActionRequested(SessionAction::Suspend));
                    },
                },

                #[name = "hibernate_tile"]
                Tile {
                    set_primary: "Hibernate",
                    #[watch]
                    set_visible: model.sections.action_visible(SessionAction::Hibernate),
                    #[wrap(Some)]
                    set_left = gtk::Image {
                        set_icon_name: Some("document-save-symbolic"),
                        set_pixel_size: 16,
                    },
                    connect_activated[sender] => move |_| {
                        let _ = sender.output(Output::ActionRequested(SessionAction::Hibernate));
                    },
                },

                #[name = "reboot_tile"]
                Tile {
                    set_primary: "Restart",
                    #[watch]
                    set_visible: model.sections.action_visible(SessionAction::Reboot),
                    #[wrap(Some)]
                    set_left = gtk::Image {
                        set_icon_name: Some("system-reboot-symbolic"),
                        set_pixel_size: 16,
                    },
                    connect_activated[sender] => move |_| {
                        let _ = sender.output(Output::ActionRequested(SessionAction::Reboot));
                    },
                },

                #[name = "power_off_tile"]
                Tile {
                    set_primary: "Shut Down",
                    #[watch]
                    set_visible: model.sections.action_visible(SessionAction::PowerOff),
                    #[wrap(Some)]
                    set_left = gtk::Image {
                        set_icon_name: Some("system-shutdown-symbolic"),
                        set_pixel_size: 16,
                    },
                    connect_activated[sender] => move |_| {
                        let _ = sender.output(Output::ActionRequested(SessionAction::PowerOff));
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
        let state = State::default();
        let sections = action_sections(&init.config, &state);
        let mut model = Popover {
            popover: AnimatedPopover::new(),
            config: init.config,
            sections,
            hero_icon_name: "avatar-default-symbolic",
            hero_subtitle: String::new(),
            state,
        };

        let widgets = view_output!();
        model.popover = widgets.root.clone();
        widgets.root.set_parent(&init.parent);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::Toggle => self.popover.toggle(),
            Input::Close => self.popover.close(),
            Input::UpdateState(state) => {
                self.hero_icon_name = hero_icon_name(&state);
                self.hero_subtitle = hero_subtitle(&state);
                self.state = state;
                self.update_actions();
            }
            Input::Reconfigure(config) => {
                self.config = config;
                self.update_actions();
            }
        }
    }
}

impl Popover {
    fn update_actions(&mut self) {
        self.sections = action_sections(&self.config, &self.state);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActionSections {
    session: Vec<ActionVisibility>,
    power: Vec<ActionVisibility>,
    session_visible: bool,
    power_visible: bool,
}

impl ActionSections {
    fn action_visible(&self, action: SessionAction) -> bool {
        self.session
            .iter()
            .chain(self.power.iter())
            .any(|item| item.action == action && item.visible)
    }

    #[cfg(test)]
    fn visible_actions(&self) -> Vec<SessionAction> {
        self.session
            .iter()
            .chain(self.power.iter())
            .filter(|item| item.visible)
            .map(|item| item.action)
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActionVisibility {
    action: SessionAction,
    visible: bool,
}

fn action_sections(config: &Config, state: &State) -> ActionSections {
    let session = build_session_items(config, state);
    let power = build_power_items(config, state);
    let session_visible = has_visible_items(&session);
    let power_visible = has_visible_items(&power);

    ActionSections {
        session,
        power,
        session_visible,
        power_visible,
    }
}

fn has_visible_items(items: &[ActionVisibility]) -> bool {
    items.iter().any(|item| item.visible)
}

fn build_session_items(config: &Config, state: &State) -> Vec<ActionVisibility> {
    vec![
        action_visibility(
            SessionAction::Lock,
            config.show_lock && action_available(&state.snapshot.capabilities.lock),
        ),
        action_visibility(
            SessionAction::Logout,
            config.show_logout
                && matches!(
                    state.snapshot.capabilities.backend,
                    SessionBackendState::Available
                ),
        ),
    ]
}

fn build_power_items(config: &Config, state: &State) -> Vec<ActionVisibility> {
    let capabilities = &state.snapshot.capabilities;
    vec![
        action_visibility(
            SessionAction::Suspend,
            config.show_suspend && action_available(&capabilities.suspend),
        ),
        action_visibility(
            SessionAction::Hibernate,
            config.show_hibernate && action_available(&capabilities.hibernate),
        ),
        action_visibility(
            SessionAction::Reboot,
            config.show_reboot && action_available(&capabilities.reboot),
        ),
        action_visibility(
            SessionAction::PowerOff,
            config.show_shutdown && action_available(&capabilities.power_off),
        ),
    ]
}

fn action_visibility(action: SessionAction, visible: bool) -> ActionVisibility {
    ActionVisibility { action, visible }
}

fn action_available(availability: &SessionActionAvailability) -> bool {
    matches!(
        availability,
        SessionActionAvailability::Available | SessionActionAvailability::Challenge
    )
}

fn hero_icon_name(state: &State) -> &'static str {
    match state.active_action {
        Some(action) => action_icon_name(action),
        None => "avatar-default-symbolic",
    }
}

fn action_icon_name(action: SessionAction) -> &'static str {
    match action {
        SessionAction::Lock => "system-lock-screen-symbolic",
        SessionAction::Logout => "system-log-out-symbolic",
        SessionAction::Suspend => "media-playback-pause-symbolic",
        SessionAction::Hibernate => "document-save-symbolic",
        SessionAction::Reboot => "system-reboot-symbolic",
        SessionAction::PowerOff => "system-shutdown-symbolic",
    }
}

fn hero_subtitle(state: &State) -> String {
    match &state.health {
        SessionServiceHealth::Degraded { message } => return message.clone(),
        SessionServiceHealth::Ready => {}
    }

    if state.active_action.is_some() {
        format::state_text(state)
    } else if state.snapshot.subtitle.is_empty() {
        state.snapshot.host_name.clone()
    } else {
        state.snapshot.subtitle.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::session::{SessionActionCapabilities, SessionSnapshot};

    #[test]
    fn action_available_accepts_challenge() {
        assert!(action_available(&SessionActionAvailability::Available));
        assert!(action_available(&SessionActionAvailability::Challenge));
        assert!(!action_available(&SessionActionAvailability::Unavailable));
    }

    #[test]
    fn builds_visible_items_from_config_and_capabilities() {
        let state = State {
            snapshot: SessionSnapshot {
                capabilities: SessionActionCapabilities {
                    backend: SessionBackendState::Available,
                    lock: SessionActionAvailability::Available,
                    suspend: SessionActionAvailability::Challenge,
                    hibernate: SessionActionAvailability::Available,
                    reboot: SessionActionAvailability::Unavailable,
                    power_off: SessionActionAvailability::Available,
                },
                ..SessionSnapshot::default()
            },
            ..State::default()
        };
        let config = Config {
            show_hibernate: true,
            ..Config::default()
        };

        let session_items = build_session_items(&config, &state);
        let power_items = build_power_items(&config, &state);

        assert!(
            session_items
                .iter()
                .any(|item| item.action == SessionAction::Lock && item.visible)
        );
        assert!(
            power_items
                .iter()
                .any(|item| item.action == SessionAction::Suspend && item.visible)
        );
        assert!(
            power_items
                .iter()
                .any(|item| item.action == SessionAction::Hibernate && item.visible)
        );
        assert!(
            power_items
                .iter()
                .any(|item| item.action == SessionAction::PowerOff && item.visible)
        );
        assert!(
            power_items
                .iter()
                .any(|item| item.action == SessionAction::Reboot && !item.visible)
        );
    }

    #[test]
    fn visible_item_helper_tracks_section_visibility() {
        let state = State::default();
        let items = build_session_items(&Config::default(), &state);

        assert!(!has_visible_items(&items));

        let state = State {
            snapshot: SessionSnapshot {
                capabilities: SessionActionCapabilities {
                    backend: SessionBackendState::Unavailable,
                    lock: SessionActionAvailability::Available,
                    suspend: SessionActionAvailability::Unavailable,
                    hibernate: SessionActionAvailability::Unavailable,
                    reboot: SessionActionAvailability::Unavailable,
                    power_off: SessionActionAvailability::Unavailable,
                },
                ..SessionSnapshot::default()
            },
            ..State::default()
        };
        let items = build_session_items(&Config::default(), &state);

        assert!(has_visible_items(&items));
    }

    #[test]
    fn action_sections_preserve_flat_session_and_power_rows() {
        let state = State {
            snapshot: SessionSnapshot {
                capabilities: SessionActionCapabilities {
                    backend: SessionBackendState::Available,
                    lock: SessionActionAvailability::Available,
                    suspend: SessionActionAvailability::Challenge,
                    hibernate: SessionActionAvailability::Available,
                    reboot: SessionActionAvailability::Unavailable,
                    power_off: SessionActionAvailability::Available,
                },
                ..SessionSnapshot::default()
            },
            ..State::default()
        };
        let config = Config {
            show_hibernate: true,
            ..Config::default()
        };

        let sections = action_sections(&config, &state);

        assert!(sections.session_visible);
        assert!(sections.power_visible);
        assert_eq!(
            sections.visible_actions(),
            vec![
                SessionAction::Lock,
                SessionAction::Logout,
                SessionAction::Suspend,
                SessionAction::Hibernate,
                SessionAction::PowerOff,
            ]
        );
    }
}
