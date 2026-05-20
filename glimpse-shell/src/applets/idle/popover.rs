use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::{
    components::popover_scroll,
    services::wayland_idle_inhibit::WaylandHealth,
    widgets::{
        animated_popover::AnimatedPopover, empty_state::EmptyState, hero::Hero,
        key_value_grid::KeyValueGrid, popover_shell::PopoverShell, segmented_tile::SegmentedTile,
        tile::Tile,
    },
};

use glimpse_core::services::idle_inhibitor::{Command, IdleInhibitorRecord, SourceKind, State};

use super::format;

pub struct Popover {
    popover: AnimatedPopover,
    hero: Hero,
    hero_icon_name: &'static str,
    hero_subtitle: String,
    manual_hold_on: bool,
    updating_toggle: Rc<Cell<bool>>,
    scroller: gtk::ScrolledWindow,
    list: gtk::Box,
    empty: EmptyState,
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
    Command(Command),
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = Init;
    type Input = Input;
    type Output = Output;

    view! {
        root = AnimatedPopover {
            add_css_class: "idle-popover",
            add_css_class: "popover-size-medium",
            set_hexpand: false,
            set_autohide: true,

            PopoverShell {
                set_footer_visible: false,

                #[name = "hero"]
                Hero {
                    #[watch]
                    set_icon: Some(model.hero_icon_name),
                    set_title: "Idle Inhibitor",
                    #[watch]
                    set_subtitle: &model.hero_subtitle,
                    set_trailing_visible: true,
                    connect_toggled[toggle_guard, sender] => move |_, active| {
                        if toggle_guard.get() {
                            return;
                        }
                        sender.input(Input::SetManualHold(active));
                    },
                },

                #[name = "empty"]
                EmptyState {
                    set_title: "No inhibitors",
                    set_subtitle: Some("Nothing is preventing idle"),
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
        }
    }

    fn init(init: Init, _root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let updating_toggle = Rc::new(Cell::new(false));
        let mut model = Popover {
            popover: AnimatedPopover::new(),
            hero: Hero::new(),
            hero_icon_name: "view-conceal-symbolic",
            hero_subtitle: "Nothing is preventing idle".into(),
            manual_hold_on: false,
            updating_toggle,
            scroller: gtk::ScrolledWindow::new(),
            list: gtk::Box::new(gtk::Orientation::Vertical, 2),
            empty: EmptyState::new(),
            rows: HashMap::new(),
            own_unique_name: init.own_unique_name,
        };

        let toggle_guard = model.updating_toggle.clone();
        let widgets = view_output!();
        model.popover = widgets.root.clone();
        model.hero = widgets.hero.clone();
        model.scroller = widgets.scroller.clone();
        model.list = widgets.list.clone();
        model.empty = widgets.empty.clone();
        widgets.root.set_parent(&init.parent);
        popover_scroll::install_half_monitor_limit(
            widgets.root.upcast_ref(),
            &widgets.scroller,
            &init.parent,
        );

        model.sync_hero_toggle_from_model();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Input::Toggle => self.popover.toggle(),
            Input::UpdateState {
                state,
                wayland,
                daemon_offline,
            } => {
                let visible_records = visible_records(&state.inhibitors);
                self.set_manual_hold_on(
                    state
                        .inhibitors
                        .iter()
                        .any(|r| r.bus_name == self.own_unique_name),
                );
                // Hero icon mirrors the panel icon: tracks the user's manual
                // hold, not the total inhibitor count. Otherwise on a system
                // with any persistent external inhibitor (niri's power-key
                // handler etc.) it would never flip.
                self.hero_icon_name = if self.manual_hold_on {
                    "view-reveal-symbolic"
                } else {
                    "view-conceal-symbolic"
                };
                self.hero_subtitle = format::subtitle(&format::SubtitleInputs {
                    daemon_offline,
                    wayland: &wayland,
                    backend: &state.health,
                    records: &visible_records,
                    own_unique_name: &self.own_unique_name,
                });
                self.sync_rows(&visible_records, &sender);
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
}

impl Popover {
    fn set_manual_hold_on(&mut self, on: bool) {
        self.manual_hold_on = on;
        self.sync_hero_toggle_from_model();
    }

    fn sync_hero_toggle_from_model(&self) {
        self.updating_toggle.set(true);
        self.hero.set_toggle_active(self.manual_hold_on);
        self.updating_toggle.set(false);
    }

    fn sync_rows(&mut self, records: &[IdleInhibitorRecord], sender: &ComponentSender<Self>) {
        let mut seen: HashSet<u64> = HashSet::new();
        let mut previous: Option<gtk::Widget> = None;
        let mut has_rows = false;

        for record in records {
            has_rows = true;
            seen.insert(record.id);
            let is_self = record.bus_name == self.own_unique_name;

            let entry = self.rows.entry(record.id);
            let row = entry.or_insert_with(|| IdleListRow::new(record, is_self, sender));
            row.update(record, is_self);
            place_row(row, &self.list, previous.as_ref());
            previous = Some(row.widget().clone());
        }

        // Remove rows whose records are gone.
        self.rows.retain(|id, row| {
            let keep = seen.contains(id);
            if !keep {
                let widget = row.widget();
                if let Some(parent) = widget.parent()
                    && let Ok(parent) = parent.downcast::<gtk::Box>()
                {
                    parent.remove(widget);
                }
            }
            keep
        });

        self.scroller.set_visible(has_rows);
        self.list.set_visible(has_rows);
        self.empty.set_visible(records.is_empty());
    }
}

fn visible_records(records: &[IdleInhibitorRecord]) -> Vec<IdleInhibitorRecord> {
    records
        .iter()
        .filter(|record| record.can_release)
        .cloned()
        .collect()
}

// ─── Row ────────────────────────────────────────────────────────────────

struct IdleListRow {
    root: SegmentedTile,
    icon: gtk::Image,
    details: KeyValueGrid,
    release: Tile,
    id: u64,
}

impl IdleListRow {
    fn new(record: &IdleInhibitorRecord, is_self: bool, sender: &ComponentSender<Popover>) -> Self {
        let root = SegmentedTile::new();
        root.add_css_class("idle-row");

        let icon = gtk::Image::from_icon_name(&pick_icon(record, is_self));
        icon.add_css_class("idle-row__icon");
        icon.set_pixel_size(16);
        root.set_left(Some(icon.clone()));

        let details = KeyValueGrid::new();
        let release = Tile::new();
        release.set_primary("Release");
        release.set_secondary(None);
        release.connect_activated({
            let sender = sender.clone();
            let id = record.id;
            move |_| sender.input(Input::EmitCommand(Command::Release { id }))
        });

        let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
        content.append(&details);
        content.append(&release);
        root.set_child(Some(content));

        let mut row = Self {
            root,
            icon,
            details,
            release,
            id: record.id,
        };
        row.update(record, is_self);
        row
    }

    fn update(&mut self, record: &IdleInhibitorRecord, is_self: bool) {
        debug_assert_eq!(self.id, record.id);
        self.icon.set_icon_name(Some(&pick_icon(record, is_self)));
        self.root.set_primary(&format::row_label(record));
        self.root.set_secondary(row_secondary(record).as_deref());
        self.details.clear();
        for (key, value) in detail_rows(record) {
            self.details.add_row(key, &value);
        }
        self.release
            .set_tooltip_text(Some(&format!("Release {}", format::row_label(record))));
    }

    fn widget(&self) -> &gtk::Widget {
        self.root.upcast_ref()
    }
}

fn place_row(row: &IdleListRow, container: &gtk::Box, previous: Option<&gtk::Widget>) {
    let row_widget = row.widget();
    let target = container.clone().upcast::<gtk::Widget>();
    let already_in_container = row_widget.parent().is_some_and(|parent| parent == target);

    if !already_in_container {
        if let Some(parent) = row_widget.parent()
            && let Ok(parent) = parent.downcast::<gtk::Box>()
        {
            parent.remove(row_widget);
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

fn row_secondary(r: &IdleInhibitorRecord) -> Option<String> {
    if r.why.is_empty() {
        describe_source(r)
    } else {
        Some(r.why.clone())
    }
}

fn detail_rows(r: &IdleInhibitorRecord) -> Vec<(&'static str, String)> {
    let mut rows = Vec::new();

    if !r.why.is_empty() {
        rows.push(("Reason", r.why.clone()));
    }

    let targets = describe_targets(&r.targets);
    if !targets.is_empty() {
        rows.push(("Targets", targets));
    }

    if let Some(source) = describe_source(r) {
        rows.push(("Source", source));
    }

    let identity = if !r.bus_name.is_empty() {
        r.bus_name.clone()
    } else if let SourceKind::Login1 = r.source.kind {
        format!("pid {}", r.source.pid)
    } else {
        String::new()
    };
    if !identity.is_empty() {
        rows.push(("Identity", identity));
    }

    rows.push(("Active for", format::relative_time(r.added_at_unix)));

    rows
}

fn describe_targets(t: &glimpse_core::services::idle_inhibitor::InhibitionTargets) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if t.idle {
        parts.push("idle");
    }
    if t.suspend {
        parts.push("suspend");
    }
    if t.shutdown {
        parts.push("shutdown");
    }
    if t.lid_switch {
        parts.push("lid");
    }
    if t.power_key {
        parts.push("power-key");
    }
    if t.suspend_key {
        parts.push("suspend-key");
    }
    if t.hibernate_key {
        parts.push("hibernate-key");
    }
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
    fn detail_rows_include_record_context() {
        let r = rec(1, ":1.99", "Firefox", true);
        let rows = detail_rows(&r);

        assert!(
            rows.iter()
                .any(|(key, value)| { *key == "Reason" && value == "Playing video" })
        );
        assert!(
            rows.iter()
                .any(|(key, value)| { *key == "Targets" && value == "idle" })
        );
        assert!(
            rows.iter()
                .any(|(key, value)| { *key == "Identity" && value == ":1.99" })
        );
    }

    #[test]
    fn visible_records_filters_readonly_inhibitors() {
        let records = vec![rec(1, ":1.99", "Firefox", true), rec(2, "", "apt", false)];

        let visible = visible_records(&records);

        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, 1);
    }

    #[test]
    fn detail_rows_for_login1_records_use_pid_identity() {
        let mut r = rec(1, "", "apt", false);
        r.source = IdleInhibitorSource::login1(4242, 0, Login1Mode::Block);
        let rows = detail_rows(&r);

        assert!(
            rows.iter()
                .any(|(key, value)| { *key == "Identity" && value == "pid 4242" })
        );
        assert!(
            rows.iter()
                .any(|(key, value)| { *key == "Source" && value == "Source: systemd-inhibit" })
        );
    }
}
