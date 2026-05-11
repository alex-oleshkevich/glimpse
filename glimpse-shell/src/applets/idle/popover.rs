#![allow(unused_assignments)]
// Consumed by the idle applet controller in Task 16 (glimpse-37w.16).
#![allow(dead_code)]

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, glib, prelude::*},
};
use std::cell::Cell;
use std::rc::Rc;

use crate::components::{
    animated_popover::AnimatedPopover,
    device_list::{
        ChipTier, DeviceList, DeviceListAction, DeviceListChip, DeviceListInit, DeviceListInput,
        DeviceListItem,
    },
    hero::HeroView,
    popover_scroll,
    popover_shell::PopoverShell,
};
use crate::services::wayland_idle_inhibit::WaylandHealth;

use glimpse_core::services::idle_inhibitor::{Command, IdleInhibitorRecord, SourceKind, State};

use super::format;

pub struct Popover {
    animation: AnimatedPopover,
    hero_icon_name: &'static str,
    hero_subtitle: String,
    manual_hold_on: bool,
    updating_toggle: Rc<Cell<bool>>,
    devices: Controller<DeviceList<Command>>,
    device_items: Vec<DeviceListItem<Command>>,
    own_unique_name: String,
}

pub struct Init {
    pub parent: gtk::Box,
    pub own_unique_name: String,
}

#[derive(Debug, Clone)]
pub enum Input {
    Toggle,
    Close,
    UpdateState {
        state: State,
        wayland: WaylandHealth,
        daemon_offline: bool,
    },
    SetManualHold(bool),
    DeviceCommand(Command),
}

#[derive(Debug, Clone)]
pub enum Output {
    Opened,
    Closed,
    Command(Command),
}

#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = Init;
    type Input = Input;
    type Output = Output;

    view! {
        root = gtk::Popover {
            add_css_class: "idle-popover",
            add_css_class: "popover-size-medium",
            set_hexpand: false,

            #[template]
            PopoverShell {
                #[template_child]
                footer { set_visible: false, },

                #[template_child]
                content {
                    #[name = "hero"]
                    #[template]
                    HeroView {
                        #[template_child]
                        trailing { set_visible: true, },
                    },

                    gtk::Separator {
                        set_orientation: gtk::Orientation::Horizontal,
                    },

                    #[name = "scroller"]
                    gtk::ScrolledWindow {
                        set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                        set_vexpand: false,
                        set_propagate_natural_height: true,

                        #[local_ref]
                        devices_widget -> gtk::Box {},
                    },
                },
            },
        }
    }

    fn init(init: Init, _root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let devices = DeviceList::builder()
            .launch(DeviceListInit {
                header: None,
                items: Vec::new(),
            })
            .forward(sender.input_sender(), Input::DeviceCommand);
        let devices_widget = devices.widget().clone();

        let updating_toggle = Rc::new(Cell::new(false));
        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);
        popover_scroll::install_half_monitor_limit(&widgets.root, &widgets.scroller, &init.parent);
        devices.widget().set_visible(false);

        let guard = updating_toggle.clone();
        let toggle_sender = sender.clone();
        widgets.hero.toggle.connect_state_set(move |sw, active| {
            if guard.get() {
                return glib::Propagation::Stop;
            }
            // Hold the visual state at the user's selection while the D-Bus
            // round-trip to the daemon happens; otherwise GTK reverts it
            // before the service echoes back and the toggle "bounces."
            sw.set_state(active);
            toggle_sender.input(Input::SetManualHold(active));
            glib::Propagation::Stop
        });

        let opened_sender = sender.clone();
        widgets.root.connect_show(move |_| {
            let _ = opened_sender.output(Output::Opened);
        });
        let closed_sender = sender.clone();
        widgets.root.connect_closed(move |_| {
            let _ = closed_sender.output(Output::Closed);
        });

        let model = Popover {
            animation: AnimatedPopover::new(&widgets.root),
            hero_icon_name: "media-playback-pause-symbolic",
            hero_subtitle: "Nothing is preventing idle".into(),
            manual_hold_on: false,
            updating_toggle,
            devices,
            device_items: Vec::new(),
            own_unique_name: init.own_unique_name,
        };
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Input::Toggle => self.animation.toggle(),
            Input::Close => self.animation.close(),
            Input::UpdateState {
                state,
                wayland,
                daemon_offline,
            } => {
                self.manual_hold_on = state
                    .inhibitors
                    .iter()
                    .any(|r| r.bus_name == self.own_unique_name);
                self.hero_icon_name = if state.inhibitors.is_empty() {
                    "media-playback-pause-symbolic"
                } else {
                    "media-playback-start-symbolic"
                };
                self.hero_subtitle = format::subtitle(&format::SubtitleInputs {
                    daemon_offline,
                    wayland: &wayland,
                    backend: &state.health,
                    records: &state.inhibitors,
                    own_unique_name: &self.own_unique_name,
                });
                let items = build_items(&state.inhibitors, &self.own_unique_name);
                if self.device_items != items {
                    self.devices.widget().set_visible(!items.is_empty());
                    self.devices.emit(DeviceListInput::Update(items.clone()));
                    self.device_items = items;
                }
            }
            Input::SetManualHold(on) => {
                let _ = sender.output(Output::Command(Command::SetManualHold(on)));
            }
            Input::DeviceCommand(cmd) => {
                let _ = sender.output(Output::Command(cmd));
            }
        }
    }

    fn post_view() {
        hero.icon.set_icon_name(Some(model.hero_icon_name));
        hero.title.set_label("Idle Inhibitor");
        hero.subtitle.set_label(&model.hero_subtitle);
        if hero.toggle.is_active() != model.manual_hold_on {
            model.updating_toggle.set(true);
            hero.toggle.set_active(model.manual_hold_on);
            hero.toggle.set_state(model.manual_hold_on);
            model.updating_toggle.set(false);
        }
    }
}

fn build_items(
    records: &[IdleInhibitorRecord],
    own_unique_name: &str,
) -> Vec<DeviceListItem<Command>> {
    records
        .iter()
        .map(|r| build_item(r, own_unique_name))
        .collect()
}

fn build_item(r: &IdleInhibitorRecord, own_unique_name: &str) -> DeviceListItem<Command> {
    let is_self = r.bus_name == own_unique_name;
    DeviceListItem {
        id: r.id.to_string(),
        icon: pick_icon(r, is_self),
        label: format::row_label(r),
        status: format!("{} · {}", r.why, format::relative_time(r.added_at_unix)),
        busy: false,
        tooltip: Some(format!("{}\n{}", r.bus_name, r.why)),
        active: false,
        visible: true,
        command: None,
        actions: Vec::new(),
        chips: build_chips(r),
        secondary_status: format::row_secondary(r),
        primary_action: if r.can_release {
            Some(DeviceListAction {
                id: "release".into(),
                label: "Release".into(),
                destructive: true,
                enabled: true,
                visible: true,
                command: Command::Release { id: r.id },
            })
        } else {
            None
        },
    }
}

fn pick_icon(r: &IdleInhibitorRecord, is_self: bool) -> String {
    if is_self {
        return "applications-system-symbolic".into();
    }
    match r.source.kind {
        SourceKind::Portal => "application-x-flatpak-symbolic".into(),
        SourceKind::Login1 => "security-medium-symbolic".into(),
        SourceKind::ScreenSaver => "application-x-executable-symbolic".into(),
    }
}

fn build_chips(r: &IdleInhibitorRecord) -> Vec<DeviceListChip> {
    let mut v = Vec::new();
    let t = &r.targets;
    if t.idle {
        v.push(DeviceListChip {
            label: "idle".into(),
            tier: ChipTier::Primary,
        });
    }
    if t.suspend {
        v.push(DeviceListChip {
            label: "suspend".into(),
            tier: ChipTier::Primary,
        });
    }
    if t.shutdown {
        v.push(DeviceListChip {
            label: "shutdown".into(),
            tier: ChipTier::Primary,
        });
    }
    if t.lid_switch {
        v.push(DeviceListChip {
            label: "lid".into(),
            tier: ChipTier::Primary,
        });
    }
    if t.power_key {
        v.push(DeviceListChip {
            label: "power-key".into(),
            tier: ChipTier::Secondary,
        });
    }
    if t.suspend_key {
        v.push(DeviceListChip {
            label: "suspend-key".into(),
            tier: ChipTier::Secondary,
        });
    }
    if t.hibernate_key {
        v.push(DeviceListChip {
            label: "hibernate-key".into(),
            tier: ChipTier::Secondary,
        });
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::idle_inhibitor::{
        IdleInhibitorSource, InhibitionTargets, Login1Mode,
    };

    fn rec(id: u64, bus_name: &str, who: &str, can_release: bool) -> IdleInhibitorRecord {
        IdleInhibitorRecord {
            id,
            who: who.into(),
            why: "y".into(),
            bus_name: bus_name.into(),
            process_name: String::new(),
            source: IdleInhibitorSource::screen_saver(id as u32),
            targets: InhibitionTargets::idle_only(),
            can_release,
            added_at_unix: 0,
        }
    }

    #[test]
    fn release_action_present_iff_can_release() {
        let r = rec(1, ":1.99", "Firefox", true);
        let item = build_item(&r, ":1.7");
        assert!(item.primary_action.is_some());

        let mut r2 = rec(2, "", "apt", false);
        r2.source = IdleInhibitorSource::login1(42, 0, Login1Mode::Block);
        let item2 = build_item(&r2, ":1.7");
        assert!(item2.primary_action.is_none());
    }

    #[test]
    fn own_record_uses_glimpse_icon() {
        let r = rec(1, ":1.7", "Glimpse", true);
        let item = build_item(&r, ":1.7");
        assert_eq!(item.icon, "applications-system-symbolic");
    }

    #[test]
    fn record_with_glimpse_who_but_foreign_bus_does_not_trigger_self() {
        let r = rec(1, ":1.99", "Glimpse", true);
        let item = build_item(&r, ":1.7");
        assert_ne!(item.icon, "applications-system-symbolic");
    }

    #[test]
    fn primary_chips_match_targets() {
        let mut r = rec(1, ":1.99", "x", true);
        r.targets.suspend = true;
        r.targets.power_key = true;
        let item = build_item(&r, ":1.7");
        let primaries: Vec<&str> = item
            .chips
            .iter()
            .filter(|c| matches!(c.tier, ChipTier::Primary))
            .map(|c| c.label.as_str())
            .collect();
        assert!(primaries.contains(&"idle"));
        assert!(primaries.contains(&"suspend"));
        let secondaries: Vec<&str> = item
            .chips
            .iter()
            .filter(|c| matches!(c.tier, ChipTier::Secondary))
            .map(|c| c.label.as_str())
            .collect();
        assert!(secondaries.contains(&"power-key"));
    }
}
