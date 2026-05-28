use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk::prelude::*};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use glimpse_core::compositors::{CompositorType, Monitor, Workspace};
use glimpse_core::services::{
    compositor::{Command, CompositorHandle, State},
    framework::ServiceCommand,
};

use crate::panels::applets::AppletConfig;
use crate::widgets::panel_indicator::PanelIndicator;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub label_format: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            label_format: "{name_or_index}".into(),
        }
    }
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };
        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid workspace applet config, using defaults");
                Self::default()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct View {
    visible: bool,
    label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkspaceState {
    compositor: CompositorType,
    workspaces_available: bool,
    current_workspace: Option<usize>,
    workspaces: Vec<Workspace>,
    monitors: Vec<Monitor>,
}

impl WorkspaceState {
    fn effective_current_workspace(&self, panel_monitor: Option<&str>) -> Option<usize> {
        if let Some(name) = panel_monitor {
            if let Some(active) = self
                .monitors
                .iter()
                .find(|m| m.name == name)
                .and_then(|m| m.active_workspace)
            {
                return Some(active);
            }
        }
        self.current_workspace
    }
}

impl From<&State> for WorkspaceState {
    fn from(state: &State) -> Self {
        Self {
            compositor: state.compositor,
            workspaces_available: state.capabilities.workspaces,
            current_workspace: state.current_workspace,
            workspaces: state.workspaces.clone(),
            monitors: state.monitors.clone(),
        }
    }
}

fn view_from_state(config: &Config, state: &WorkspaceState, panel_monitor: Option<&str>) -> View {
    if !state.workspaces_available {
        return View {
            visible: false,
            label: String::new(),
        };
    }
    let current = state.effective_current_workspace(panel_monitor);
    let workspace = current.and_then(|id| state.workspaces.iter().find(|w| w.id == id));
    let fallback = current.unwrap_or(1);
    let label = format_workspace_label(&config.label_format, state.compositor, workspace, fallback);
    View {
        visible: true,
        label,
    }
}

fn scroll_direction(dx: f64, dy: f64) -> Option<bool> {
    if dx == 0.0 && dy == 0.0 {
        return None;
    }
    if dx.abs() > dy.abs() {
        Some(dx > 0.0)
    } else {
        Some(dy > 0.0)
    }
}

pub struct Applet {
    config: Config,
    state: WorkspaceState,
    view: View,
    service: CompositorHandle,
    subscription_cancel: CancellationToken,
    panel_monitor: Option<String>,
}

pub struct Init {
    pub service: CompositorHandle,
    pub config: Config,
    pub panel_monitor: Option<String>,
}

#[derive(Debug)]
pub enum Input {
    ServiceStateChanged(State),
    Reconfigure(Config),
    Scroll { next: bool },
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for Applet {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        root = PanelIndicator {
            #[watch]
            set_visible: model.view.visible,
            #[watch]
            set_label: if model.view.label.is_empty() {
                None
            } else {
                Some(model.view.label.as_str())
            },
            connect_scrolled[sender] => move |_, dx, dy| {
                if let Some(next) = scroll_direction(dx, dy) {
                    sender.input(Input::Scroll { next });
                }
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let state = WorkspaceState::from(&init.service.snapshot());
        let view = view_from_state(&init.config, &state, init.panel_monitor.as_deref());
        let model = Applet {
            config: init.config,
            state,
            view,
            service: init.service,
            subscription_cancel: CancellationToken::new(),
            panel_monitor: init.panel_monitor,
        };

        let service = model.service.clone();
        let cancel = model.subscription_cancel.clone();
        let subscription_sender = sender.input_sender().clone();
        relm4::spawn(async move {
            let mut sub = service.subscribe();
            if subscription_sender
                .send(Input::ServiceStateChanged(sub.borrow().clone()))
                .is_err()
            {
                return;
            }
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    changed = sub.changed() => {
                        if changed.is_err() { break; }
                        if subscription_sender
                            .send(Input::ServiceStateChanged(sub.borrow().clone()))
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::ServiceStateChanged(state) => {
                let state = WorkspaceState::from(&state);
                if self.state != state {
                    self.state = state;
                    self.sync_view();
                }
            }
            Input::Reconfigure(config) => {
                self.config = config;
                self.sync_view();
            }
            Input::Scroll { next } => {
                self.send_command(if next {
                    Command::FocusNextWorkspace
                } else {
                    Command::FocusPreviousWorkspace
                });
            }
        }
    }
}

impl Applet {
    fn sync_view(&mut self) {
        let view = view_from_state(&self.config, &self.state, self.panel_monitor.as_deref());
        if self.view != view {
            self.view = view;
        }
    }

    fn send_command(&self, command: Command) {
        let service = self.service.clone();
        relm4::spawn(async move {
            if let Err(error) = service.send(ServiceCommand::Command(command)).await {
                tracing::warn!(%error, "failed to send compositor command from workspace applet");
            }
        });
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.subscription_cancel.cancel();
    }
}

fn effective_index(
    compositor: CompositorType,
    workspace: Option<&Workspace>,
    fallback: usize,
) -> usize {
    match compositor {
        CompositorType::Niri => workspace.and_then(|w| w.index).unwrap_or(fallback),
        _ => workspace.map(|w| w.id).unwrap_or(fallback),
    }
}

pub fn format_workspace_label(
    format: &str,
    compositor: CompositorType,
    workspace: Option<&Workspace>,
    fallback: usize,
) -> String {
    let id = workspace.map(|w| w.id).unwrap_or(fallback).to_string();
    let index = workspace
        .and_then(|w| w.index)
        .unwrap_or(fallback)
        .to_string();
    let name = workspace
        .and_then(|w| w.name.as_deref())
        .unwrap_or_default();
    let name_or_index = if !name.is_empty() {
        name.to_owned()
    } else {
        effective_index(compositor, workspace, fallback).to_string()
    };
    format
        .replace("{name_or_index}", &name_or_index)
        .replace("{name}", name)
        .replace("{index}", &index)
        .replace("{id}", &id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(id: usize, index: usize, name: Option<&str>) -> Workspace {
        Workspace {
            id,
            index: Some(index),
            name: name.map(str::to_owned),
            monitor: None,
            active: false,
            focused: false,
            urgent: false,
            active_window: None,
        }
    }

    #[test]
    fn format_prefers_name_over_index_when_name_is_set() {
        let ws = workspace(3, 3, Some("chat"));
        assert_eq!(
            format_workspace_label("{name_or_index}", CompositorType::Niri, Some(&ws), 3),
            "chat"
        );
    }

    #[test]
    fn format_falls_back_to_index_on_niri_when_name_absent() {
        let ws = workspace(7, 2, None);
        assert_eq!(
            format_workspace_label("{name_or_index}", CompositorType::Niri, Some(&ws), 1),
            "2"
        );
    }

    #[test]
    fn format_falls_back_to_id_on_hyprland_when_name_absent() {
        let ws = workspace(9, 3, None);
        assert_eq!(
            format_workspace_label("{name_or_index}", CompositorType::Hyprland, Some(&ws), 1),
            "9"
        );
    }

    #[test]
    fn format_tokens_expand_independently() {
        let ws = workspace(42, 3, Some("work"));
        let result =
            format_workspace_label("{name} {index} {id}", CompositorType::Niri, Some(&ws), 1);
        assert_eq!(result, "work 3 42");
    }

    #[test]
    fn format_uses_fallback_when_workspace_is_none() {
        assert_eq!(
            format_workspace_label("{name_or_index}", CompositorType::Niri, None, 4),
            "4"
        );
    }

    fn ws_state(workspaces: Vec<Workspace>, current: Option<usize>) -> WorkspaceState {
        WorkspaceState {
            compositor: CompositorType::Niri,
            workspaces_available: true,
            current_workspace: current,
            workspaces,
            monitors: vec![],
        }
    }

    #[test]
    fn view_hidden_when_workspaces_unavailable() {
        let mut s = ws_state(vec![], Some(1));
        s.workspaces_available = false;
        let v = view_from_state(&Config::default(), &s, None);
        assert!(!v.visible);
    }

    #[test]
    fn view_shows_current_workspace_label() {
        let ws = workspace(2, 2, Some("chat"));
        let s = ws_state(vec![ws], Some(2));
        let v = view_from_state(&Config::default(), &s, None);
        assert!(v.visible);
        assert_eq!(v.label, "chat");
    }

    #[test]
    fn view_uses_panel_monitor_workspace_over_global_current() {
        use glimpse_core::compositors::Monitor;
        let ws1 = workspace(1, 1, Some("main"));
        let ws2 = workspace(2, 2, Some("side"));
        let mut s = ws_state(vec![ws1, ws2], Some(1));
        s.monitors = vec![Monitor {
            id: None,
            name: "DP-1".into(),
            description: None,
            active_workspace: Some(2),
            focused: false,
            make: None,
            model: None,
            enabled: true,
            built_in: false,
            current_mode: None,
        }];
        let v = view_from_state(&Config::default(), &s, Some("DP-1"));
        assert_eq!(v.label, "side");
    }
}
