use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent, WidgetTemplate,
    gtk::{self, glib, prelude::*},
};

use glimpse_core::compositors::Monitor;

use crate::components::{
    animated_popover::AnimatedPopover, hero::HeroView, item::ItemView, popover_scroll,
    popover_shell::PopoverShell,
};

use super::format;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorRow {
    pub name: String,
    pub label: String,
    pub tooltip: String,
    pub enabled: bool,
}

pub struct Popover {
    animation: AnimatedPopover,
    monitors: Vec<MonitorRow>,
    suppress_toggle_emit: Rc<Cell<bool>>,
    list: gtk::Box,
    rows: HashMap<String, RowWidgets>,
    hero_subtitle: gtk::Label,
}

struct RowWidgets {
    view: ItemView,
    switch: gtk::Switch,
}

pub struct Init {
    pub parent: gtk::Box,
}

#[derive(Debug, Clone)]
pub enum Input {
    Toggle,
    StateChanged(Vec<Monitor>),
    ToggleClicked { name: String, on: bool },
}

#[derive(Debug, Clone)]
pub enum Output {
    Opened,
    Closed,
    SetEnabled { name: String, on: bool },
    LastMonitorWarning { label: String },
}

#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = Init;
    type Input = Input;
    type Output = Output;

    view! {
        root = gtk::Popover {
            add_css_class: "monitors-popover",
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
                        icon {
                            set_icon_name: Some("video-display-symbolic"),
                        },
                    },

                    gtk::Separator {
                        set_orientation: gtk::Orientation::Horizontal,
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
                        },
                    },
                },
            },
        }
    }

    fn init(init: Init, _root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let suppress_toggle_emit = Rc::new(Cell::new(false));
        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);
        popover_scroll::install_half_monitor_limit(&widgets.root, &widgets.scroller, &init.parent);

        let opened_sender = sender.clone();
        widgets.root.connect_show(move |_| {
            let _ = opened_sender.output(Output::Opened);
        });
        let closed_sender = sender.clone();
        widgets.root.connect_closed(move |_| {
            let _ = closed_sender.output(Output::Closed);
        });

        widgets.hero.title.set_label("Monitors");
        widgets.hero.subtitle.set_label("");

        let model = Popover {
            animation: AnimatedPopover::new(&widgets.root),
            monitors: Vec::new(),
            suppress_toggle_emit,
            list: widgets.list.clone(),
            rows: HashMap::new(),
            hero_subtitle: widgets.hero.subtitle.clone(),
        };
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Input::Toggle => self.animation.toggle(),
            Input::StateChanged(monitors) => {
                self.monitors = sorted_rows(&monitors);
                self.hero_subtitle
                    .set_label(&hero_subtitle(&self.monitors));
                self.suppress_toggle_emit.set(true);
                self.sync_rows(&sender);
                self.suppress_toggle_emit.set(false);
            }
            Input::ToggleClicked { name, on } => {
                if self.suppress_toggle_emit.get() {
                    return;
                }
                let guard = last_enabled_guard(&self.monitors, &name, on);
                if guard {
                    let label = self
                        .monitors
                        .iter()
                        .find(|m| m.name == name)
                        .map(|m| m.label.clone())
                        .unwrap_or_else(|| name.clone());
                    if let Some(widgets) = self.rows.get(&name) {
                        self.suppress_toggle_emit.set(true);
                        widgets.switch.set_active(true);
                        widgets.switch.set_state(true);
                        self.suppress_toggle_emit.set(false);
                    }
                    let _ = sender.output(Output::LastMonitorWarning { label });
                    return;
                }
                let _ = sender.output(Output::SetEnabled { name, on });
            }
        }
    }
}

impl Popover {
    fn sync_rows(&mut self, sender: &ComponentSender<Self>) {
        let mut previous: Option<gtk::Widget> = None;
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        for monitor in &self.monitors {
            seen.insert(monitor.name.clone());
            let widgets = self
                .rows
                .entry(monitor.name.clone())
                .or_insert_with(|| build_row(monitor, &self.suppress_toggle_emit, sender));
            update_row(widgets, monitor, &self.suppress_toggle_emit);
            place_row(&widgets.view, &self.list, previous.as_ref());
            previous = Some(widgets.view.button.clone().upcast());
        }

        self.rows.retain(|name, widgets| {
            let keep = seen.contains(name);
            if !keep {
                let widget: &gtk::Widget = widgets.view.button.upcast_ref();
                if let Some(parent) = widget.parent()
                    && let Ok(parent) = parent.downcast::<gtk::Box>()
                {
                    parent.remove(widget);
                }
            }
            keep
        });
    }
}

fn build_row(
    row: &MonitorRow,
    suppress: &Rc<Cell<bool>>,
    sender: &ComponentSender<Popover>,
) -> RowWidgets {
    let view = ItemView::init(());
    view.left.set_visible(true);
    view.right.set_visible(true);

    let icon = gtk::Image::from_icon_name("video-display-symbolic");
    icon.set_pixel_size(16);
    view.left.append(&icon);

    let switch = gtk::Switch::new();
    switch.set_valign(gtk::Align::Center);
    view.right.append(&switch);

    let name = row.name.clone();
    let guard = suppress.clone();
    let toggle_sender = sender.clone();
    switch.connect_state_set(move |sw, active| {
        if guard.get() {
            return glib::Propagation::Stop;
        }
        sw.set_state(active);
        toggle_sender.input(Input::ToggleClicked {
            name: name.clone(),
            on: active,
        });
        glib::Propagation::Stop
    });

    let widgets = RowWidgets { view, switch };
    update_row(&widgets, row, suppress);
    widgets
}

fn update_row(widgets: &RowWidgets, row: &MonitorRow, suppress: &Rc<Cell<bool>>) {
    widgets.view.label.set_label(&row.label);
    widgets.view.button.set_tooltip_text(Some(&row.tooltip));
    suppress.set(true);
    widgets.switch.set_active(row.enabled);
    widgets.switch.set_state(row.enabled);
    suppress.set(false);
}

fn place_row(view: &ItemView, container: &gtk::Box, previous: Option<&gtk::Widget>) {
    let row_widget: &gtk::Widget = view.button.upcast_ref();
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

/// Reads cached state, which may briefly be stale during hot-unplug events; the service-side guard in `glimpse-core/src/services/compositor.rs::would_disable_last` is the authoritative refusal point and logs a warning if it fires.
pub(crate) fn last_enabled_guard(monitors: &[MonitorRow], name: &str, on: bool) -> bool {
    if on {
        return false;
    }
    let enabled_count = monitors.iter().filter(|m| m.enabled).count();
    enabled_count == 1 && monitors.iter().any(|m| m.name == name && m.enabled)
}

fn hero_subtitle(rows: &[MonitorRow]) -> String {
    let total = rows.len();
    let enabled = rows.iter().filter(|m| m.enabled).count();
    match total {
        0 => "No displays detected".into(),
        1 => "1 display".into(),
        _ => format!("{enabled} of {total} enabled"),
    }
}

pub(crate) fn sorted_rows(monitors: &[Monitor]) -> Vec<MonitorRow> {
    let mut rows: Vec<MonitorRow> = monitors
        .iter()
        .map(|m| MonitorRow {
            name: m.name.clone(),
            label: format::row_label(m),
            tooltip: format::row_tooltip(m),
            enabled: m.enabled,
        })
        .collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::compositors::MonitorMode;

    fn row(name: &str, enabled: bool) -> MonitorRow {
        MonitorRow {
            name: name.into(),
            label: name.into(),
            tooltip: name.into(),
            enabled,
        }
    }

    fn mk_monitor(name: &str, model: Option<&str>, mode: Option<MonitorMode>) -> Monitor {
        Monitor {
            id: Some(1),
            name: name.into(),
            description: None,
            active_workspace: None,
            focused: false,
            make: None,
            model: model.map(str::to_string),
            enabled: true,
            built_in: false,
            current_mode: mode,
        }
    }

    #[test]
    fn last_enabled_guard_returns_true_when_target_is_only_enabled() {
        let monitors = vec![row("eDP-1", true), row("DP-2", false)];
        assert!(last_enabled_guard(&monitors, "eDP-1", false));
    }

    #[test]
    fn last_enabled_guard_returns_false_when_another_enabled() {
        let monitors = vec![row("eDP-1", true), row("DP-2", true)];
        assert!(!last_enabled_guard(&monitors, "eDP-1", false));
    }

    #[test]
    fn last_enabled_guard_returns_false_on_enable() {
        let monitors = vec![row("eDP-1", true), row("DP-2", false)];
        assert!(!last_enabled_guard(&monitors, "DP-2", true));
    }

    #[test]
    fn sorted_rows_orders_by_connector_name() {
        let monitors = vec![
            mk_monitor("HDMI-A-1", None, None),
            mk_monitor("DP-2", None, None),
            mk_monitor("eDP-1", None, None),
        ];
        let rows = sorted_rows(&monitors);
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["DP-2", "HDMI-A-1", "eDP-1"]);
    }

    #[test]
    fn sorted_rows_populates_label_and_tooltip_from_format() {
        let monitors = vec![mk_monitor(
            "DP-1",
            Some("Dell U2720Q"),
            Some(MonitorMode {
                width: 3840,
                height: 2160,
                refresh_mhz: 59997,
            }),
        )];
        let rows = sorted_rows(&monitors);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "Dell U2720Q");
        assert_eq!(rows[0].tooltip, "DP-1 \u{00b7} 3840\u{00d7}2160 @ 60 Hz");
    }
}
