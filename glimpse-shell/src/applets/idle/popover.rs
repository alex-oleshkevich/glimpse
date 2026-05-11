#![allow(unused_assignments)]

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    WidgetTemplate,
    gtk::{self, glib, prelude::*},
};
use std::cell::Cell;
use std::rc::Rc;

use crate::components::{
    animated_popover::AnimatedPopover, hero::HeroView, item::ItemView,
    popover_scroll, popover_shell::PopoverShell,
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
    rows_container: gtk::Box,
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
    EmitCommand(Command),
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

                        #[name = "rows"]
                        gtk::Box {
                            add_css_class: "idle-popover__rows",
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 0,
                        },
                    },
                },
            },
        }
    }

    fn init(init: Init, _root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let updating_toggle = Rc::new(Cell::new(false));
        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);
        popover_scroll::install_half_monitor_limit(&widgets.root, &widgets.scroller, &init.parent);
        widgets.rows.set_visible(false);

        let guard = updating_toggle.clone();
        let toggle_sender = sender.clone();
        widgets.hero.toggle.connect_state_set(move |sw, active| {
            if guard.get() {
                return glib::Propagation::Stop;
            }
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
            rows_container: widgets.rows.clone(),
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
                rebuild_rows(
                    &self.rows_container,
                    &state.inhibitors,
                    &self.own_unique_name,
                    &sender,
                );
                self.rows_container
                    .set_visible(!state.inhibitors.is_empty());
            }
            Input::SetManualHold(on) => {
                self.manual_hold_on = on;
                let _ = sender.output(Output::Command(Command::SetManualHold(on)));
            }
            Input::EmitCommand(cmd) => {
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

fn rebuild_rows(
    container: &gtk::Box,
    records: &[IdleInhibitorRecord],
    own_unique_name: &str,
    sender: &ComponentSender<Popover>,
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    for r in records {
        container.append(&build_row(r, own_unique_name, sender));
    }
}

fn build_row(
    r: &IdleInhibitorRecord,
    own_unique_name: &str,
    sender: &ComponentSender<Popover>,
) -> gtk::Widget {
    let is_self = r.bus_name == own_unique_name;
    let item = ItemView::init(());

    // Icon
    item.left.set_visible(true);
    let icon = gtk::Image::from_icon_name(&pick_icon(r, is_self));
    icon.set_pixel_size(20);
    item.left.append(&icon);

    // Label
    item.label.set_label(&format::row_label(r));

    // Tooltip carries everything else.
    item.button.set_tooltip_text(Some(&build_tooltip(r)));

    // Release button
    if r.can_release {
        item.right.set_visible(true);
        let release = gtk::Button::with_label("Release");
        release.add_css_class("destructive-action");
        release.add_css_class("flat");
        release.set_valign(gtk::Align::Center);
        let cmd = Command::Release { id: r.id };
        let sender = sender.clone();
        release.connect_clicked(move |_| {
            sender.input(Input::EmitCommand(cmd.clone()));
        });
        item.right.append(&release);
    }

    // The row itself doesn't act on click; the inline Release button handles
    // the only meaningful action. Disable button-as-action by suppressing focus.
    item.button.set_can_focus(false);
    item.button.set_sensitive(true);

    item.button.upcast()
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

fn build_tooltip(r: &IdleInhibitorRecord) -> String {
    let mut lines: Vec<String> = Vec::new();

    if !r.why.is_empty() {
        lines.push(r.why.clone());
    }

    let targets = describe_targets(&r.targets);
    if !targets.is_empty() {
        lines.push(format!("Targets: {targets}"));
    }

    if let Some(source) = describe_source(r) {
        lines.push(source);
    }

    let identity = if !r.bus_name.is_empty() {
        r.bus_name.clone()
    } else if let SourceKind::Login1 = r.source.kind {
        format!("pid {}", r.source.pid)
    } else {
        String::new()
    };
    if !identity.is_empty() {
        lines.push(identity);
    }

    lines.push(format!(
        "Active for {}",
        format::relative_time(r.added_at_unix)
    ));

    lines.join("\n")
}

fn describe_targets(t: &glimpse_core::services::idle_inhibitor::InhibitionTargets) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if t.idle { parts.push("idle"); }
    if t.suspend { parts.push("suspend"); }
    if t.shutdown { parts.push("shutdown"); }
    if t.lid_switch { parts.push("lid"); }
    if t.power_key { parts.push("power-key"); }
    if t.suspend_key { parts.push("suspend-key"); }
    if t.hibernate_key { parts.push("hibernate-key"); }
    parts.join(", ")
}

fn describe_source(r: &IdleInhibitorRecord) -> Option<String> {
    match r.source.kind {
        SourceKind::ScreenSaver => Some("Source: ScreenSaver".into()),
        SourceKind::Portal => Some(format!("Source: Flatpak via portal ({})", r.source.app_id)),
        SourceKind::Login1 => Some("Source: systemd-inhibit".into()),
    }
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
            why: "Playing video".into(),
            bus_name: bus_name.into(),
            process_name: String::new(),
            source: IdleInhibitorSource::screen_saver(id as u32),
            targets: InhibitionTargets::idle_only(),
            can_release,
            added_at_unix: 0,
        }
    }

    #[test]
    fn pick_icon_distinguishes_sources_and_self() {
        let r_self = rec(1, ":1.7", "Glimpse", true);
        assert_eq!(pick_icon(&r_self, true), "applications-system-symbolic");

        let mut r_portal = rec(2, ":1.99", "org.mozilla.firefox", true);
        r_portal.source = IdleInhibitorSource::portal("org.mozilla.firefox".into(), "/r/1".into());
        assert_eq!(pick_icon(&r_portal, false), "application-x-flatpak-symbolic");

        let mut r_logind = rec(3, "", "apt", false);
        r_logind.source = IdleInhibitorSource::login1(42, 0, Login1Mode::Block);
        assert_eq!(pick_icon(&r_logind, false), "security-medium-symbolic");
    }

    #[test]
    fn tooltip_carries_why_targets_source_identity_and_relative_time() {
        let mut r = rec(1, ":1.99", "Firefox", true);
        r.targets.suspend = true;
        let tip = build_tooltip(&r);
        assert!(tip.contains("Playing video"));
        assert!(tip.contains("Targets: idle, suspend"));
        assert!(tip.contains("Source: ScreenSaver"));
        assert!(tip.contains(":1.99"));
        assert!(tip.contains("Active for"));
    }

    #[test]
    fn tooltip_for_login1_records_uses_pid_identity() {
        let mut r = rec(1, "", "apt", false);
        r.source = IdleInhibitorSource::login1(4242, 0, Login1Mode::Block);
        let tip = build_tooltip(&r);
        assert!(tip.contains("pid 4242"));
        assert!(tip.contains("Source: systemd-inhibit"));
    }
}
