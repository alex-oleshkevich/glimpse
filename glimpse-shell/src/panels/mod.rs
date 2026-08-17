use std::collections::HashMap;

use gtk4::gdk;
use gtk4::prelude::{BoxExt, GtkWindowExt, OrientableExt, WidgetExt};
use gtk4_layer_shell::LayerShell;
use relm4::{Component, ComponentParts, ComponentSender, gtk};

pub mod applets;

use crate::{
    panels::applets::{AppletController, AppletKey, build_applets, reconcile_applets},
    theme,
};
use glimpse_core::ipc::IpcEmitter;
use glimpse_core::services::framework::Services;
use glimpse_core::{AppletConfig, PanelConfig, Position};

#[derive(Debug, PartialEq, Clone, Eq, Hash)]
pub struct PanelKey {
    pub index: usize,
    pub monitor: String,
    pub position: Position,
}

pub struct Init {
    pub config: PanelConfig,
    pub services: Services,
    pub monitor: Option<gdk::Monitor>,
    pub monitor_connector: Option<String>,
    pub applet_configs: HashMap<String, AppletConfig>,
    pub ipc: IpcEmitter,
}

#[derive(Debug)]
pub enum Input {
    Reconfigure(PanelRuntimeConfig),
    /// Toggles a popover by applet config name. Sends a reply exactly once,
    /// and only when this panel owns a matching applet — callers that
    /// broadcast to every panel treat silence as "not found here".
    TogglePopover {
        applet: String,
        section: Option<PanelSection>,
        occurrence: Option<usize>,
        reply: tokio::sync::mpsc::UnboundedSender<Result<(), String>>,
    },
}

#[derive(Debug)]
pub struct PanelRuntimeConfig {
    pub config: PanelConfig,
    pub applet_configs: HashMap<String, AppletConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PanelSection {
    Left,
    Center,
    Right,
}

pub struct Panel {
    services: Services,
    monitor_connector: Option<String>,
    applet_configs: HashMap<String, AppletConfig>,
    ipc: IpcEmitter,
    dynamic_container: gtk::Box,
    left: SectionState,
    center: SectionState,
    right: SectionState,
}

struct SectionState {
    container: gtk::Box,
    applets: HashMap<AppletKey, AppletController>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelEndChild {
    Dynamic,
    Right,
}

fn dynamic_island_css_classes() -> [&'static str; 2] {
    ["island", "island-dynamic"]
}

fn dynamic_island_initially_visible() -> bool {
    false
}

fn end_group_order() -> [PanelEndChild; 2] {
    [PanelEndChild::Dynamic, PanelEndChild::Right]
}

fn append_end_group_children(end_box: &gtk::Box, dynamic_box: &gtk::Box, right_box: &gtk::Box) {
    for child in end_group_order() {
        match child {
            PanelEndChild::Dynamic => end_box.append(dynamic_box),
            PanelEndChild::Right => end_box.append(right_box),
        }
    }
}

#[relm4::component(pub)]
impl Component for Panel {
    type Init = Init;
    type Input = Input;
    type Output = ();
    type CommandOutput = ();

    view! {
        gtk::Window {
            set_decorated: false,
            add_css_class: "panel",

            #[local_ref]
            layout -> gtk::CenterBox {
                set_hexpand: true,
                set_orientation: orientation_for_position(&init.config.position),
                set_start_widget: Some(&left_box),
                set_center_widget: Some(&center_box),
                set_end_widget: Some(&end_box),
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        tracing::info!(
            "configuring panel, position {:?}, {} applets",
            init.config.position,
            init.config.left.len() + init.config.center.len() + init.config.right.len()
        );
        init_layer_shell(&root);
        if let Some(monitor) = init.monitor.as_ref() {
            root.set_monitor(Some(monitor));
        }
        apply_panel_config(&root, &init.config);
        theme::apply_theme_mode(&root, &init.config.theme_mode);

        let layout_orientation = orientation_for_position(&init.config.position);
        let left_box = gtk::Box::builder()
            .css_classes(vec!["island", "island-start"])
            .orientation(layout_orientation)
            .valign(gtk::Align::Center)
            .build();
        let center_box = gtk::Box::builder()
            .css_classes(vec!["island", "island-center"])
            .orientation(layout_orientation)
            .valign(gtk::Align::Center)
            .build();
        let right_box = gtk::Box::builder()
            .css_classes(vec!["island", "island-end"])
            .orientation(layout_orientation)
            .valign(gtk::Align::Center)
            .build();
        let dynamic_box = gtk::Box::builder()
            .css_classes(dynamic_island_css_classes().to_vec())
            .orientation(layout_orientation)
            .valign(gtk::Align::Center)
            .visible(dynamic_island_initially_visible())
            .build();
        let end_box = gtk::Box::builder()
            .orientation(layout_orientation)
            .valign(gtk::Align::Center)
            .build();
        append_end_group_children(&end_box, &dynamic_box, &right_box);
        let layout = gtk::CenterBox::new();

        let ipc = init.ipc;
        let panel_monitor = init.monitor_connector.clone().unwrap_or_default();
        let left_applets = build_applets(
            PanelSection::Left,
            &init.config.left,
            &left_box,
            &dynamic_box,
            &init.applet_configs,
            init.services.clone(),
            init.monitor_connector.as_deref(),
            init.config.theme_mode,
            &ipc,
            &panel_monitor,
        );
        let center_applets = build_applets(
            PanelSection::Center,
            &init.config.center,
            &center_box,
            &dynamic_box,
            &init.applet_configs,
            init.services.clone(),
            init.monitor_connector.as_deref(),
            init.config.theme_mode,
            &ipc,
            &panel_monitor,
        );
        let right_applets = build_applets(
            PanelSection::Right,
            &init.config.right,
            &right_box,
            &dynamic_box,
            &init.applet_configs,
            init.services.clone(),
            init.monitor_connector.as_deref(),
            init.config.theme_mode,
            &ipc,
            &panel_monitor,
        );
        let widgets = view_output!();
        let model = Panel {
            services: init.services,
            monitor_connector: init.monitor_connector,
            applet_configs: init.applet_configs,
            ipc,
            dynamic_container: dynamic_box,
            left: SectionState {
                container: left_box,
                applets: left_applets,
            },
            center: SectionState {
                container: center_box,
                applets: center_applets,
            },
            right: SectionState {
                container: right_box,
                applets: right_applets,
            },
        };

        root.present();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>, root: &Self::Root) {
        match message {
            Input::Reconfigure(runtime) => {
                tracing::debug!("panel config change, updating");
                apply_panel_config(root, &runtime.config);
                theme::apply_theme_mode(root, &runtime.config.theme_mode);

                let panel_monitor = self.monitor_connector.clone().unwrap_or_default();
                reconcile_applets(
                    PanelSection::Left,
                    &runtime.config.left,
                    &self.left.container,
                    &self.dynamic_container,
                    &mut self.left.applets,
                    &self.applet_configs,
                    &runtime.applet_configs,
                    self.services.clone(),
                    self.monitor_connector.as_deref(),
                    runtime.config.theme_mode,
                    &self.ipc,
                    &panel_monitor,
                );
                reconcile_applets(
                    PanelSection::Center,
                    &runtime.config.center,
                    &self.center.container,
                    &self.dynamic_container,
                    &mut self.center.applets,
                    &self.applet_configs,
                    &runtime.applet_configs,
                    self.services.clone(),
                    self.monitor_connector.as_deref(),
                    runtime.config.theme_mode,
                    &self.ipc,
                    &panel_monitor,
                );
                reconcile_applets(
                    PanelSection::Right,
                    &runtime.config.right,
                    &self.right.container,
                    &self.dynamic_container,
                    &mut self.right.applets,
                    &self.applet_configs,
                    &runtime.applet_configs,
                    self.services.clone(),
                    self.monitor_connector.as_deref(),
                    runtime.config.theme_mode,
                    &self.ipc,
                    &panel_monitor,
                );

                self.applet_configs = runtime.applet_configs;
            }
            Input::TogglePopover {
                applet,
                section,
                occurrence,
                reply,
            } => {
                let occurrence = occurrence.unwrap_or(0);
                if let Some(result) =
                    self.find_and_toggle_popover(&applet, section.as_ref(), occurrence)
                {
                    let _ = reply.send(result);
                }
            }
        }
    }
}

impl Panel {
    fn find_and_toggle_popover(
        &self,
        applet: &str,
        section: Option<&PanelSection>,
        occurrence: usize,
    ) -> Option<Result<(), String>> {
        let sections = [
            (PanelSection::Left, &self.left.applets),
            (PanelSection::Center, &self.center.applets),
            (PanelSection::Right, &self.right.applets),
        ];
        for (panel_section, applets) in sections {
            if section.is_some_and(|s| s != &panel_section) {
                continue;
            }
            if let Some((_, controller)) = applets
                .iter()
                .find(|(key, _)| key.name == applet && key.occurrence == occurrence)
            {
                return Some(if controller.toggle_popover() {
                    Ok(())
                } else {
                    Err(format!("applet '{applet}' has no popover"))
                });
            }
        }
        None
    }
}

fn init_layer_shell(window: &gtk::Window) {
    window.init_layer_shell();
    window.set_layer(gtk4_layer_shell::Layer::Top);
    window.set_namespace(Some("glimpse-panel"));
    window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
    window.auto_exclusive_zone_enable();
}

fn apply_panel_config(window: &gtk::Window, config: &PanelConfig) {
    window.set_margin(gtk4_layer_shell::Edge::Top, config.margin.top);
    window.set_margin(gtk4_layer_shell::Edge::Right, config.margin.right);
    window.set_margin(gtk4_layer_shell::Edge::Bottom, config.margin.bottom);
    window.set_margin(gtk4_layer_shell::Edge::Left, config.margin.left);
    window.set_anchor(gtk4_layer_shell::Edge::Top, false);
    window.set_anchor(gtk4_layer_shell::Edge::Right, false);
    window.set_anchor(gtk4_layer_shell::Edge::Bottom, false);
    window.set_anchor(gtk4_layer_shell::Edge::Left, false);

    match config.position {
        Position::Top | Position::Bottom => {
            window.set_height_request(config.size);
            window.set_width_request(1);
            window.add_css_class("panel-horizontal");
        }
        Position::Left | Position::Right => {
            window.set_height_request(1);
            window.set_width_request(config.size);
            window.add_css_class("panel-vertical");
        }
    }

    match config.position {
        Position::Top => {
            window.set_anchor(gtk4_layer_shell::Edge::Top, true);
            window.set_anchor(gtk4_layer_shell::Edge::Left, true);
            window.set_anchor(gtk4_layer_shell::Edge::Right, true);
        }
        Position::Right => {
            window.set_anchor(gtk4_layer_shell::Edge::Top, true);
            window.set_anchor(gtk4_layer_shell::Edge::Right, true);
            window.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
        }
        Position::Bottom => {
            window.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
            window.set_anchor(gtk4_layer_shell::Edge::Left, true);
            window.set_anchor(gtk4_layer_shell::Edge::Right, true);
        }
        Position::Left => {
            window.set_anchor(gtk4_layer_shell::Edge::Top, true);
            window.set_anchor(gtk4_layer_shell::Edge::Left, true);
            window.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
        }
    }
}

fn orientation_for_position(position: &Position) -> gtk::Orientation {
    match position {
        Position::Top | Position::Bottom => gtk::Orientation::Horizontal,
        Position::Left | Position::Right => gtk::Orientation::Vertical,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_island_uses_island_classes_and_starts_hidden() {
        assert_eq!(dynamic_island_css_classes(), ["island", "island-dynamic"]);
        assert!(!dynamic_island_initially_visible());
    }

    #[test]
    fn end_group_orders_dynamic_island_before_right_island() {
        assert_eq!(
            end_group_order(),
            [PanelEndChild::Dynamic, PanelEndChild::Right]
        );
    }
}
