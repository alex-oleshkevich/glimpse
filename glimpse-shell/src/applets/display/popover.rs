use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
    time::{Duration, Instant},
};

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::{
    utils::popover_scroll,
    widgets::{
        animated_popover::AnimatedPopover, empty_state::EmptyState, hero::Hero,
        popover_shell::PopoverShell, switch_tile::SwitchTile,
    },
};
use glimpse_core::compositors::Monitor;

use super::format;

const MONITOR_TOGGLE_ECHO_GRACE: Duration = Duration::from_secs(2);

pub struct Popover {
    popover: AnimatedPopover,
    monitors: Vec<Monitor>,
    list: gtk::Box,
    rows: HashMap<String, MonitorRow>,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    UpdateMonitors(Vec<Monitor>),
    SetMonitorEnabled { name: String, on: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopoverOutput {
    SetMonitorEnabled { name: String, on: bool },
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = PopoverOutput;

    view! {
        root = AnimatedPopover {
            add_css_class: "popover-size-medium",

            PopoverShell {

                Hero {
                    set_icon: Some("video-display-symbolic"),
                    set_title: "Displays",
                    #[watch]
                    set_subtitle: &format::hero_subtitle(&model.monitors),
                },

                EmptyState {
                    set_title: "No displays",
                    set_subtitle: Some("No monitor information available"),
                    #[watch]
                    set_visible: model.monitors.is_empty(),
                },

                #[name = "scroller"]
                gtk::ScrolledWindow {
                    set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                    set_vexpand: false,
                    set_propagate_natural_height: true,
                    #[watch]
                    set_visible: !model.monitors.is_empty(),

                    #[name = "list"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 2,
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut model = Popover {
            popover: AnimatedPopover::new(),
            monitors: Vec::new(),
            list: gtk::Box::new(gtk::Orientation::Vertical, 2),
            rows: HashMap::new(),
        };

        let widgets = view_output!();
        model.popover = widgets.root.clone();
        model.list = widgets.list.clone();

        widgets.root.set_parent(&init.parent);
        popover_scroll::install_half_monitor_limit(
            widgets.root.upcast_ref(),
            &widgets.scroller,
            &init.parent,
        );

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => {
                self.popover.toggle();
            }
            PopoverInput::UpdateMonitors(monitors) => {
                self.monitors = monitors;
                self.sync_rows(&sender);
            }
            PopoverInput::SetMonitorEnabled { name, on } => {
                let _ = sender.output(PopoverOutput::SetMonitorEnabled { name, on });
            }
        }
    }
}

impl Popover {
    fn sync_rows(&mut self, sender: &ComponentSender<Self>) {
        let models = monitor_row_models(&self.monitors);
        let mut seen = HashSet::new();
        let mut previous: Option<gtk::Widget> = None;

        for model in models {
            seen.insert(model.name.clone());
            let row = self
                .rows
                .entry(model.name.clone())
                .or_insert_with(|| MonitorRow::new(&model, sender));
            row.update(&model);
            place_row(row.widget(), &self.list, previous.as_ref());
            previous = Some(row.widget().clone());
        }

        self.rows.retain(|id, row| {
            let keep = seen.contains(id);
            if !keep {
                remove_row(row.widget());
            }
            keep
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MonitorRowModel {
    name: String,
    label: String,
    icon: &'static str,
    active: bool,
    sensitive: bool,
}

struct MonitorRow {
    root: SwitchTile,
    icon: gtk::Image,
    name: Rc<RefCell<String>>,
    updating: Rc<Cell<bool>>,
    pending_enabled: Rc<RefCell<Option<PendingMonitorToggle>>>,
}

#[derive(Debug, Clone)]
struct PendingMonitorToggle {
    enabled: bool,
    changed_at: Instant,
}

impl PendingMonitorToggle {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            changed_at: Instant::now(),
        }
    }
}

impl MonitorRow {
    fn new(model: &MonitorRowModel, sender: &ComponentSender<Popover>) -> Self {
        let root = SwitchTile::new();
        root.add_css_class("display-row");

        let icon = gtk::Image::from_icon_name(model.icon);
        icon.set_pixel_size(16);
        icon.set_halign(gtk::Align::Center);
        icon.set_valign(gtk::Align::Center);
        root.set_left(Some(icon.clone()));

        let name = Rc::new(RefCell::new(model.name.clone()));
        let updating = Rc::new(Cell::new(false));
        let pending_enabled = Rc::new(RefCell::new(None));
        root.connect_toggled({
            let name = name.clone();
            let updating = updating.clone();
            let pending_enabled = pending_enabled.clone();
            let sender = sender.clone();
            move |_, active| {
                if updating.get() {
                    return;
                }
                pending_enabled
                    .borrow_mut()
                    .replace(PendingMonitorToggle::new(active));
                sender.input(PopoverInput::SetMonitorEnabled {
                    name: name.borrow().clone(),
                    on: active,
                });
            }
        });

        let row = Self {
            root,
            icon,
            name,
            updating,
            pending_enabled,
        };
        row.update(model);
        row
    }

    fn update(&self, model: &MonitorRowModel) {
        self.name.replace(model.name.clone());
        self.icon.set_icon_name(Some(model.icon));
        self.root.set_primary(&model.label);
        self.root.set_secondary(None);
        self.root.set_sensitive(model.sensitive);
        let status = if model.active { "Enabled" } else { "Disabled" };
        self.root
            .set_tooltip_text(Some(&format!("{} - {} - {status}", model.label, model.name)));
        let active = {
            let mut pending = self.pending_enabled.borrow_mut();
            monitor_enabled_for_update(&mut pending, model.active, Instant::now())
        };
        self.updating.set(true);
        self.root.set_active(active);
        self.updating.set(false);
    }

    fn widget(&self) -> &gtk::Widget {
        self.root.upcast_ref()
    }
}

fn monitor_row_models(monitors: &[Monitor]) -> Vec<MonitorRowModel> {
    if monitors.is_empty() {
        return Vec::new();
    }
    let enabled_count = monitors.iter().filter(|m| m.enabled).count();
    monitors
        .iter()
        .map(|monitor| MonitorRowModel {
            name: monitor.name.clone(),
            label: format::monitor_display_name(monitor),
            icon: monitor_icon(monitor),
            active: monitor.enabled,
            sensitive: !monitor.enabled || enabled_count > 1,
        })
        .collect()
}

fn monitor_icon(monitor: &Monitor) -> &'static str {
    if monitor.built_in {
        "computer-symbolic"
    } else {
        "video-display-symbolic"
    }
}

fn place_row(row_widget: &gtk::Widget, container: &gtk::Box, previous: Option<&gtk::Widget>) {
    let target = container.clone().upcast::<gtk::Widget>();
    let already_in_container = row_widget.parent().is_some_and(|parent| parent == target);
    if !already_in_container {
        remove_row(row_widget);
        container.append(row_widget);
    }
    container.reorder_child_after(row_widget, previous);
}

fn remove_row(row_widget: &gtk::Widget) {
    if let Some(parent) = row_widget.parent()
        && let Ok(parent) = parent.downcast::<gtk::Box>()
    {
        parent.remove(row_widget);
    }
}

fn monitor_enabled_for_update(
    pending: &mut Option<PendingMonitorToggle>,
    compositor_enabled: bool,
    now: Instant,
) -> bool {
    let Some(value) = pending else {
        return compositor_enabled;
    };

    if value.enabled == compositor_enabled {
        *pending = None;
        return compositor_enabled;
    }

    if now.duration_since(value.changed_at) < MONITOR_TOGGLE_ECHO_GRACE {
        return value.enabled;
    }

    *pending = None;
    compositor_enabled
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(name: &str, enabled: bool, built_in: bool) -> Monitor {
        Monitor {
            id: None,
            name: name.into(),
            description: None,
            active_workspace: None,
            focused: false,
            make: None,
            model: None,
            enabled,
            built_in,
            current_mode: None,
        }
    }

    #[test]
    fn monitor_rows_hidden_when_no_monitors() {
        assert!(monitor_row_models(&[]).is_empty());
    }

    #[test]
    fn single_enabled_monitor_row_is_insensitive() {
        let rows = monitor_row_models(&[monitor("eDP-1", true, true)]);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].active);
        assert!(!rows[0].sensitive);
    }

    #[test]
    fn disabled_monitor_row_is_always_sensitive() {
        let mut off = monitor("DP-2", false, false);
        off.make = Some("Dell Inc.".into());
        off.model = Some("AW2725Q".into());
        let rows = monitor_row_models(&[monitor("eDP-1", true, true), off]);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].label, "Built-in display");
        assert!(!rows[0].sensitive);
        assert_eq!(rows[1].label, "Dell Inc. AW2725Q");
        assert!(rows[1].sensitive);
    }

    #[test]
    fn pending_toggle_ignores_stale_compositor_value() {
        let now = Instant::now();
        let mut pending = Some(PendingMonitorToggle { enabled: false, changed_at: now });
        assert!(!monitor_enabled_for_update(&mut pending, true, now));
        assert!(pending.is_some());
    }

    #[test]
    fn pending_toggle_clears_when_compositor_catches_up() {
        let now = Instant::now();
        let mut pending = Some(PendingMonitorToggle { enabled: false, changed_at: now });
        assert!(!monitor_enabled_for_update(&mut pending, false, now));
        assert!(pending.is_none());
    }

    #[test]
    fn pending_toggle_expires_after_grace_period() {
        let now = Instant::now();
        let mut pending = Some(PendingMonitorToggle {
            enabled: false,
            changed_at: now - MONITOR_TOGGLE_ECHO_GRACE - Duration::from_millis(1),
        });
        assert!(monitor_enabled_for_update(&mut pending, true, now));
        assert!(pending.is_none());
    }
}
