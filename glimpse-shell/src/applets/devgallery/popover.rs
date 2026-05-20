use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::{
    components::animated_popover::AnimatedPopover,
    widgets::{
        badge::{Badge, BadgeKind}, boxed_list::BoxedList, button_row::ButtonRow,
        choice_list::ChoiceList, column::Column, expander_tile::ExpanderTile, header::Header,
        hero::Hero, key_value_grid::KeyValueGrid, popover_shell::PopoverShell, row::Row,
        segmented_tile::SegmentedTile, slider_tile::SliderTile,
        status_dot::{StatusDot, StatusDotStatus}, switch_tile::SwitchTile, tile::Tile,
    },
};

pub struct Popover {
    animation: AnimatedPopover,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopoverOutput {
    Opened,
    Closed,
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = PopoverOutput;

    view! {
        root = gtk::Popover {
            add_css_class: "devgallery-popover",
            add_css_class: "popover-size-medium",
            set_hexpand: false,

            PopoverShell {
                set_footer_visible: false,

                Column {

                        Header {
                            set_label: "Tile",
                        },

                        Tile {
                            set_primary: "Activatable row",
                            set_secondary: Some("Click me"),
                            connect_activated => move |_| {},
                        },

                        Header {
                            set_label: "SwitchTile",
                        },

                        SwitchTile {
                            set_primary: "Toggle setting",
                            set_secondary: Some("Enable or disable"),
                            connect_toggled => move |_, _| {},
                        },

                        Header {
                            set_label: "ExpanderTile",
                        },

                        ExpanderTile {
                            set_primary: "Expandable section",
                            set_secondary: Some("Click to expand"),
                            #[wrap(Some)]
                            set_child = BoxedList {
                                Tile {
                                    set_primary: "Detail one",
                                    set_secondary: Some("Value A"),
                                },
                                Tile {
                                    set_primary: "Detail two",
                                    set_secondary: Some("Value B"),
                                },
                            },
                        },

                        Header {
                            set_label: "ChoiceList",
                        },

                        ChoiceList {
                            add_choice: ("one",   "Option one",   Some("First choice"),  None),
                            add_choice: ("two",   "Option two",   Some("Second choice"), None),
                            add_choice: ("three", "Option three", Some("Third choice"),  None),
                            set_active: "two",
                            connect_changed => move |_, _| {},
                        },

                        Header {
                            set_label: "SegmentedTile",
                        },

                        SegmentedTile {
                            set_primary: "Wi-Fi",
                            set_secondary: Some("Home Network"),
                            #[wrap(Some)]
                            set_child = BoxedList {
                                Tile {
                                    set_primary: "IP Address",
                                    set_secondary: Some("192.168.1.42"),
                                },
                                Tile {
                                    set_primary: "Security",
                                    set_secondary: Some("WPA2"),
                                },
                            },
                            connect_activated => move |_| {},
                            connect_expanded => move |_, _| {},
                        },

                        Header {
                            set_label: "Row",
                        },

                        Row {
                            gtk::Image {
                                set_icon_name: Some("weather-clear-symbolic"),
                                set_pixel_size: 16,
                            },
                            gtk::Label {
                                set_label: "Sunny, 24°C",
                                set_hexpand: true,
                            },
                            gtk::Image {
                                set_icon_name: Some("go-next-symbolic"),
                                set_pixel_size: 16,
                            },
                        },

                        Header {
                            set_label: "Column",
                        },

                        Column {
                            gtk::Label {
                                set_label: "Primary text",
                                set_xalign: 0.0,
                            },
                            gtk::Label {
                                add_css_class: "dim-label",
                                add_css_class: "caption",
                                set_label: "Secondary text below",
                                set_xalign: 0.0,
                            },
                        },

                        Header {
                            set_label: "SliderTile",
                        },

                        SliderTile {
                            set_value: 0.5,
                            connect_changed => move |_, _| {},
                        },

                        SliderTile {
                            set_label: Some("Volume"),
                            #[wrap(Some)]
                            set_left = gtk::Image {
                                set_icon_name: Some("audio-volume-high-symbolic"),
                                set_pixel_size: 16,
                            },
                            set_value: 0.7,
                            connect_changed => move |_, _| {},
                        },

                        Header {
                            set_label: "KeyValueGrid",
                        },

                        KeyValueGrid {
                            add_row: ("Hostname", "arch-laptop"),
                            add_row: ("Kernel",   "6.8.0-lts"),
                            add_row: ("Uptime",   "3h 42m"),
                            add_row: ("IP",       "192.168.1.42"),
                        },

                        Header {
                            set_label: "ButtonRow",
                        },

                        ButtonRow {
                            gtk::Button {
                                set_icon_name: "media-skip-backward-symbolic",
                            },
                            gtk::Button {
                                set_icon_name: "media-playback-start-symbolic",
                            },
                            gtk::Button {
                                set_icon_name: "media-skip-forward-symbolic",
                            },
                        },

                        Header {
                            set_label: "StatusDot",
                        },

                        Row {
                            StatusDot {
                                set_status: StatusDotStatus::Success,
                            },
                            StatusDot {
                                set_status: StatusDotStatus::Warning,
                            },
                            StatusDot {
                                set_status: StatusDotStatus::Error,
                            },
                            StatusDot {
                                set_status: StatusDotStatus::Accent,
                            },
                            StatusDot {
                                set_status: StatusDotStatus::Neutral,
                            },
                        },

                        Header {
                            set_label: "Badge",
                        },

                        Row {
                            Badge {
                                set_label: "Default",
                            },
                            Badge {
                                set_label: "Success",
                                set_kind: BadgeKind::Success,
                            },
                            Badge {
                                set_label: "Warning",
                                set_kind: BadgeKind::Warning,
                            },
                            Badge {
                                set_label: "Error",
                                set_kind: BadgeKind::Error,
                            },
                            Badge {
                                set_label: "Accent",
                                set_kind: BadgeKind::Accent,
                            },
                        },

                        Header {
                            set_label: "Hero",
                        },

                        Hero {
                            set_icon: Some("weather-clear-symbolic"),
                            set_title: "Clear Sky",
                            set_subtitle: "24°C · Feels like 22°C",
                        },

                        Hero {
                            set_icon: Some("network-wireless-symbolic"),
                            set_title: "Wi-Fi",
                            set_subtitle: "Home Network",
                            set_trailing_visible: true,
                            set_toggle_active: true,
                            connect_toggled => move |_, _| {},
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
        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);

        let opened_sender = sender.clone();
        widgets.root.connect_show(move |_| {
            let _ = opened_sender.output(PopoverOutput::Opened);
        });

        let closed_sender = sender.clone();
        widgets.root.connect_closed(move |_| {
            let _ = closed_sender.output(PopoverOutput::Closed);
        });

        let model = Popover {
            animation: AnimatedPopover::new(&widgets.root),
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => self.animation.toggle(),
        }
    }
}
