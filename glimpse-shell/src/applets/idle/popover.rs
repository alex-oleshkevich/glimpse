#![allow(unused_assignments)]

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent, WidgetTemplate,
    gtk::{self, gio, glib, prelude::*},
};

use crate::components::{
    animated_popover::AnimatedPopover, hero::HeroView, item::ItemView, popover_scroll,
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
    list: gtk::Box,
    empty: gtk::Box,
    rows: HashMap<u64, IdleListRow>,
    own_unique_name: String,
}

pub struct Init {
    pub parent: gtk::Box,
    pub own_unique_name: String,
}

#[derive(Debug, Clone)]
pub enum Input {
    Toggle,
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

                    #[name = "empty"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 4,
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Center,
                        set_vexpand: true,
                        set_hexpand: true,
                        add_css_class: "empty-state",

                        gtk::Label {
                            add_css_class: "empty-state__title",
                            set_label: "Nothing is preventing idle",
                        },
                    },

                    #[name = "scroller"]
                    gtk::ScrolledWindow {
                        set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                        set_vexpand: true,
                        set_propagate_natural_height: true,

                        #[name = "list"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 2,
                            add_css_class: "idle-list",
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
            hero_icon_name: "view-conceal-symbolic",
            hero_subtitle: "Nothing is preventing idle".into(),
            manual_hold_on: false,
            updating_toggle,
            list: widgets.list.clone(),
            empty: widgets.empty.clone(),
            rows: HashMap::new(),
            own_unique_name: init.own_unique_name,
        };
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Input::Toggle => self.animation.toggle(),
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
                    "view-conceal-symbolic"
                } else {
                    "view-reveal-symbolic"
                };
                self.hero_subtitle = format::subtitle(&format::SubtitleInputs {
                    daemon_offline,
                    wayland: &wayland,
                    backend: &state.health,
                    records: &state.inhibitors,
                    own_unique_name: &self.own_unique_name,
                });
                self.sync_rows(&state.inhibitors, &sender);
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

impl Popover {
    fn sync_rows(
        &mut self,
        records: &[IdleInhibitorRecord],
        sender: &ComponentSender<Self>,
    ) {
        let mut seen: HashSet<u64> = HashSet::new();
        let mut previous: Option<gtk::Widget> = None;

        for record in records {
            seen.insert(record.id);
            let is_self = record.bus_name == self.own_unique_name;

            let entry = self.rows.entry(record.id);
            let row = entry.or_insert_with(|| {
                IdleListRow::new(record, is_self, sender)
            });
            row.update(record, is_self);
            place_row(row, &self.list, previous.as_ref());
            previous = Some(row.root.button.clone().upcast());
        }

        // Remove rows whose records are gone.
        self.rows.retain(|id, row| {
            let keep = seen.contains(id);
            if !keep {
                let widget: &gtk::Widget = row.root.button.upcast_ref();
                if let Some(parent) = widget.parent() {
                    if let Ok(parent) = parent.downcast::<gtk::Box>() {
                        parent.remove(widget);
                    }
                }
            }
            keep
        });

        let has_records = !records.is_empty();
        self.list.set_visible(has_records);
        self.empty.set_visible(!has_records);
    }
}

// ─── Row ────────────────────────────────────────────────────────────────
//
// Uses the project-wide ItemView widget_template:
//   [left slot: icon] [label (flex spacer)] [right slot: action]
// wrapped in a gtk::Button so the whole row is clickable.

struct IdleListRow {
    root: ItemView,
    icon: gtk::Image,
    /// Attached only for releasable records. Kept alive so unparent on drop works.
    _context_menu: Option<gtk::PopoverMenu>,
    id: u64,
}

impl IdleListRow {
    fn new(record: &IdleInhibitorRecord, is_self: bool, sender: &ComponentSender<Popover>) -> Self {
        let root = ItemView::init(());
        root.button.add_css_class("idle-row");
        root.left.set_visible(true);

        let icon = gtk::Image::from_icon_name(&pick_icon(record, is_self));
        icon.add_css_class("idle-row__icon");
        icon.set_pixel_size(16);
        root.left.append(&icon);

        let context_menu = if record.can_release {
            Some(install_release_menu(&root, record.id, sender))
        } else {
            None
        };

        let mut row = Self {
            root,
            icon,
            _context_menu: context_menu,
            id: record.id,
        };
        row.update(record, is_self);
        row
    }

    fn update(&mut self, record: &IdleInhibitorRecord, is_self: bool) {
        debug_assert_eq!(self.id, record.id);
        self.icon.set_icon_name(Some(&pick_icon(record, is_self)));
        self.root.label.set_label(&format::row_label(record));
        self.root.button.set_tooltip_text(Some(&build_tooltip(record)));
        // Read-only Login1 rows: button insensitive (menu install was skipped).
        self.root.button.set_sensitive(record.can_release);
    }
}

/// Install a PopoverMenu with a single "Release" item on the row's button.
/// Primary-click and right-click both open it. Returns the menu so the row
/// can keep it alive (menus get unparented on row drop).
fn install_release_menu(
    row: &ItemView,
    id: u64,
    sender: &ComponentSender<Popover>,
) -> gtk::PopoverMenu {
    let action_group = gio::SimpleActionGroup::new();
    let release = gio::SimpleAction::new("release", None);
    release.connect_activate({
        let sender = sender.clone();
        move |_, _| sender.input(Input::EmitCommand(Command::Release { id }))
    });
    action_group.add_action(&release);
    row.button
        .insert_action_group("idle-row", Some(&action_group));

    let menu = gio::Menu::new();
    menu.append(Some("Release"), Some("idle-row.release"));

    let context_menu = gtk::PopoverMenu::from_model(Some(&menu));
    context_menu.add_css_class("idle-row-menu");
    context_menu.set_parent(&row.button);
    context_menu.set_has_arrow(false);

    // Primary-click on the row body: only meaningful action is Release, so
    // surfacing the menu on left-click matches "click the row to do the
    // thing" while still giving a confirmation step.
    row.button.connect_clicked({
        let context_menu = context_menu.clone();
        move |_| context_menu.popup()
    });

    // Right-click also opens the menu (clipboard's pattern).
    let click = gtk::GestureClick::new();
    click.set_button(gtk::gdk::BUTTON_SECONDARY);
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    click.connect_pressed({
        let context_menu = context_menu.clone();
        move |gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            context_menu.popup();
        }
    });
    row.button.add_controller(click);

    row.button.connect_destroy({
        let context_menu = context_menu.clone();
        move |_| context_menu.unparent()
    });

    context_menu
}

fn place_row(row: &IdleListRow, container: &gtk::Box, previous: Option<&gtk::Widget>) {
    let row_widget: &gtk::Widget = row.root.button.upcast_ref();
    let target = container.clone().upcast::<gtk::Widget>();
    let already_in_container = row_widget.parent().is_some_and(|parent| parent == target);

    if !already_in_container {
        if let Some(parent) = row_widget.parent() {
            if let Ok(parent) = parent.downcast::<gtk::Box>() {
                parent.remove(row_widget);
            }
        }
        container.append(row_widget);
    }
    container.reorder_child_after(row_widget, previous);
}

// ─── Helpers ────────────────────────────────────────────────────────────

fn pick_icon(r: &IdleInhibitorRecord, is_self: bool) -> String {
    if is_self {
        return "applications-system-symbolic".into();
    }
    match r.source.kind {
        SourceKind::Portal => "package-x-generic-symbolic".into(),
        SourceKind::Login1 => "emblem-system-symbolic".into(),
        SourceKind::ScreenSaver => "preferences-desktop-screensaver-symbolic".into(),
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

    if r.can_release {
        lines.push(String::new());
        lines.push("Click to release".into());
    }

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
        assert_eq!(pick_icon(&r_portal, false), "package-x-generic-symbolic");

        let mut r_logind = rec(3, "", "apt", false);
        r_logind.source = IdleInhibitorSource::login1(42, 0, Login1Mode::Block);
        assert_eq!(pick_icon(&r_logind, false), "emblem-system-symbolic");
    }

    #[test]
    fn tooltip_includes_click_to_release_when_can_release_is_true() {
        let r = rec(1, ":1.99", "Firefox", true);
        let tip = build_tooltip(&r);
        assert!(tip.contains("Click to release"));
    }

    #[test]
    fn tooltip_skips_click_to_release_for_readonly_rows() {
        let mut r = rec(1, "", "apt", false);
        r.source = IdleInhibitorSource::login1(4242, 0, Login1Mode::Block);
        let tip = build_tooltip(&r);
        assert!(!tip.contains("Click to release"));
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
