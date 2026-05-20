use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::{
    services::{battery::BatteryStatus, power::PowerProfiles},
    widgets::{
        animated_popover::AnimatedPopover, battery_hero::BatteryHero, choice_list::ChoiceList,
        header::Header, key_value_grid::KeyValueGrid, popover_shell::PopoverShell,
    },
};

use super::format;

pub struct Popover {
    popover: AnimatedPopover,
    details_grid: KeyValueGrid,
    profiles_list: ChoiceList,
    hero_icon_name: String,
    hero_percentage: String,
    hero_progress: f64,
    hero_state: String,
    details: Vec<DetailRow>,
    profiles: PowerProfiles,
    degraded_visible: bool,
    degraded_text: String,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    UpdateStatus(BatteryStatus),
    UpdateProfiles(PowerProfiles),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopoverOutput {
    Opened,
    Closed,
    SetProfile(String),
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = PopoverOutput;

    view! {
        root = AnimatedPopover {
            add_css_class: "battery-popover",
            add_css_class: "popover-size-small",
            set_hexpand: false,
            set_autohide: true,

            PopoverShell {
                set_footer_visible: false,

                BatteryHero {
                    #[watch]
                    set_icon_name: &model.hero_icon_name,
                    #[watch]
                    set_percentage: &model.hero_percentage,
                    #[watch]
                    set_fraction: model.hero_progress,
                    #[watch]
                    set_state: &model.hero_state,
                },

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                #[name = "details_grid"]
                KeyValueGrid {},

                #[name = "profiles_separator"]
                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                    #[watch]
                    set_visible: model.profiles_visible(),
                },

                Header {
                    set_label: "Power profile",
                    #[watch]
                    set_visible: model.profiles_visible(),
                },

                #[name = "profiles_list"]
                ChoiceList {
                    #[watch]
                    set_visible: model.profiles_visible(),
                    connect_changed[sender] => move |_, profile| {
                        let _ = sender.output(PopoverOutput::SetProfile(profile.to_owned()));
                    },
                },

                #[name = "degraded"]
                gtk::Box {
                    add_css_class: "profile-degraded-row",
                    add_css_class: "is-warning",
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 6,
                    #[watch]
                    set_visible: model.degraded_visible,

                    gtk::Image {
                        set_icon_name: Some("dialog-warning-symbolic"),
                        set_pixel_size: 14,
                    },

                    gtk::Label {
                        add_css_class: "profile-degraded",
                        set_halign: gtk::Align::Start,
                        set_wrap: true,
                        #[watch]
                        set_label: &model.degraded_text,
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
        let status = BatteryStatus::default();
        let mut model = Popover {
            popover: AnimatedPopover::new(),
            details_grid: KeyValueGrid::new(),
            profiles_list: ChoiceList::new(),
            hero_icon_name: status.icon_name.clone(),
            hero_percentage: format::percent(status.percentage),
            hero_progress: battery_fraction(&status),
            hero_state: format::state_text(&status),
            details: detail_rows(status),
            profiles: PowerProfiles::default(),
            degraded_visible: false,
            degraded_text: String::new(),
        };

        let widgets = view_output!();
        model.popover = widgets.root.clone();
        model.details_grid = widgets.details_grid.clone();
        model.profiles_list = widgets.profiles_list.clone();
        widgets.root.set_parent(&init.parent);

        let opened_sender = sender.clone();
        widgets.root.connect_show(move |_| {
            let _ = opened_sender.output(PopoverOutput::Opened);
        });

        let closed_sender = sender.clone();
        widgets.root.connect_closed(move |_| {
            let _ = closed_sender.output(PopoverOutput::Closed);
        });

        model.sync_details();
        model.sync_profiles();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            PopoverInput::Toggle => {
                self.popover.toggle();
            }
            PopoverInput::UpdateStatus(status) => {
                self.hero_icon_name = status.icon_name.clone();
                self.hero_percentage = format::percent(status.percentage);
                self.hero_progress = battery_fraction(&status);
                self.hero_state = format::state_text(&status);
                self.details = detail_rows(status);
                self.sync_details();
            }
            PopoverInput::UpdateProfiles(profiles) => {
                self.degraded_visible = !profiles.performance_degraded.is_empty();
                self.degraded_text = format::degraded_warning(&profiles.performance_degraded);
                self.profiles = profiles;
                self.sync_profiles();
            }
        }
    }
}

impl Popover {
    fn profiles_visible(&self) -> bool {
        self.profiles
            .available
            .iter()
            .any(|profile| !profile.is_empty())
    }

    fn sync_details(&self) {
        self.details_grid.clear();
        for row in self.details.iter().filter(|row| row.visible) {
            self.details_grid.add_row(row.label, &row.value);
        }
    }

    fn sync_profiles(&self) {
        self.profiles_list.clear_choices();
        for profile in self
            .profiles
            .available
            .iter()
            .filter(|profile| !profile.is_empty())
        {
            self.profiles_list.add_choice(
                profile,
                format::profile_label(profile),
                None,
                Some(format::profile_icon(profile)),
            );
        }
        if !self.profiles.active.is_empty() {
            self.profiles_list.set_active(&self.profiles.active);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct DetailRow {
    label: &'static str,
    value: String,
    visible: bool,
}

fn detail_rows(status: BatteryStatus) -> Vec<DetailRow> {
    vec![
        DetailRow {
            label: "Health",
            value: format::percent(status.capacity),
            visible: true,
        },
        DetailRow {
            label: "Model",
            value: format::optional_model(status.model),
            visible: true,
        },
        DetailRow {
            label: "Charge limit",
            value: format::percent(status.charge_threshold),
            visible: status.charge_threshold > 0,
        },
        DetailRow {
            label: "Rate",
            value: format::power_rate(status.energy_rate),
            visible: status.energy_rate > 0.0,
        },
    ]
}

fn battery_fraction(status: &BatteryStatus) -> f64 {
    status.percentage as f64 / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_rows_hide_empty_optional_values() {
        let rows = detail_rows(BatteryStatus {
            capacity: 92.0,
            model: String::new(),
            charge_threshold: 0,
            energy_rate: 0.0,
            ..BatteryStatus::default()
        });

        assert!(rows.iter().any(|row| row.label == "Health" && row.visible));
        assert!(rows.iter().any(|row| row.label == "Model" && row.visible));
        assert!(
            rows.iter()
                .any(|row| row.label == "Charge limit" && !row.visible)
        );
        assert!(rows.iter().any(|row| row.label == "Rate" && !row.visible));
    }

    #[test]
    fn battery_fraction_uses_percentage_ratio() {
        let status = BatteryStatus {
            percentage: 73,
            ..BatteryStatus::default()
        };

        assert_eq!(battery_fraction(&status), 0.73);
    }
}
