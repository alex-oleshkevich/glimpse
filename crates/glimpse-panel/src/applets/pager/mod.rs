mod label;

use std::cell::RefCell;
use std::rc::Rc;

use glimpse_config::{Applet as AppletConfig, PagerConfig, PagerMode, PagerScope, PagerShape};
use glimpse_contracts::{
    CompositorWindows, CompositorWorkspaces, FocusWindow, FocusWorkspace, Message, WindowInfo,
    WindowRef, WorkspaceInfo, WorkspaceRef,
};
use glimpse_widgets::{Focus, Pager as Strip, Shape, Slot};
use gtk4::prelude::*;

use crate::applet::{Applet, Caller, Ctx, Input, payload};
use label::{Facts, render};

pub struct Pager {
    strip: Strip,
    settings: Rc<RefCell<PagerConfig>>,
    output: Option<String>,
    workspaces: Vec<WorkspaceInfo>,
    windows: Vec<WindowInfo>,
}

impl Applet for Pager {
    fn topics(&self) -> &'static [&'static str] {
        &[CompositorWorkspaces::NAME, CompositorWindows::NAME]
    }

    fn start() -> Self {
        Self {
            strip: Strip::new(),
            settings: Rc::new(RefCell::new(PagerConfig::default())),
            output: None,
            workspaces: Vec::new(),
            windows: Vec::new(),
        }
    }

    fn view(&mut self, ctx: &Ctx) -> Option<gtk4::Widget> {
        self.output = ctx.output().map(str::to_owned);

        self.strip.connect_activated({
            let caller = ctx.caller();
            let settings = self.settings.clone();
            move |_, id| focus(&caller, settings.borrow().mode, id)
        });

        self.strip.connect_stepped({
            let caller = ctx.caller();
            let settings = self.settings.clone();
            move |_, horizontal, forward| {
                step(&caller, settings.borrow().mode, horizontal, forward);
            }
        });

        Some(self.strip.clone().upcast())
    }

    fn orient(&mut self, orientation: gtk4::Orientation) {
        self.strip.set_orientation(orientation);
    }

    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletConfig::Pager(settings) = config else {
            return;
        };
        self.settings.replace(settings.clone());
        self.render();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        let Input::Topic(event) = input else {
            return;
        };

        if let Some(update) = payload::<CompositorWorkspaces>(event) {
            self.workspaces = update.workspaces;
        } else if let Some(update) = payload::<CompositorWindows>(event) {
            self.windows = update.windows;
        } else {
            return;
        }

        self.render();
    }
}

impl Pager {
    fn render(&self) {
        let settings = self.settings.borrow();
        self.strip.set_shape(match settings.shape {
            PagerShape::Dots => Shape::Dots,
            PagerShape::Numbers => Shape::Numbers,
        });
        self.strip.set_slots(&self.slots(&settings));
    }

    fn slots(&self, settings: &PagerConfig) -> Vec<Slot> {
        let scoped: Vec<&WorkspaceInfo> = self
            .workspaces
            .iter()
            .filter(|workspace| in_scope(workspace, settings.scope, self.output.as_deref()))
            .collect();

        match settings.mode {
            PagerMode::Workspaces => scoped
                .iter()
                .map(|workspace| workspace_slot(settings, workspace))
                .collect(),
            PagerMode::Windows => {
                let showing: Vec<u64> = scoped
                    .iter()
                    .filter(|workspace| workspace.active)
                    .map(|workspace| workspace.id)
                    .collect();

                self.windows
                    .iter()
                    .filter(|window| window.workspace.is_some_and(|id| showing.contains(&id)))
                    .enumerate()
                    .map(|(position, window)| window_slot(settings, position, window))
                    .collect()
            }
        }
    }
}

fn template(settings: &PagerConfig, focused: bool) -> &str {
    match focused {
        true => &settings.focused_label,
        false => &settings.label,
    }
}

fn workspace_slot(settings: &PagerConfig, workspace: &WorkspaceInfo) -> Slot {
    let facts = Facts {
        index: workspace.index.map(u64::from),
        id: workspace.id,
        name: workspace.name.as_deref(),
    };

    Slot {
        id: workspace.id,
        label: render(template(settings, workspace.focused), &facts),
        tooltip: render("{name-or-index}", &facts),
        focus: match (workspace.focused, workspace.active) {
            (true, _) => Focus::Here,
            (false, true) => Focus::Elsewhere,
            (false, false) => Focus::None,
        },
        occupied: workspace.windows > 0,
        urgent: workspace.urgent,
    }
}

fn window_slot(settings: &PagerConfig, position: usize, window: &WindowInfo) -> Slot {
    let facts = Facts {
        index: Some(position as u64 + 1),
        id: window.id,
        name: window.app_id.as_deref(),
    };

    Slot {
        id: window.id,
        label: render(template(settings, window.focused), &facts),
        tooltip: window
            .title
            .clone()
            .or_else(|| window.app_id.clone())
            .unwrap_or_default(),
        focus: match window.focused {
            true => Focus::Here,
            false => Focus::None,
        },
        occupied: true,
        urgent: window.urgent,
    }
}

fn in_scope(workspace: &WorkspaceInfo, scope: PagerScope, output: Option<&str>) -> bool {
    match scope {
        PagerScope::Current => workspace.focused,
        PagerScope::Session => true,
        PagerScope::Output => match output {
            Some(connector) => workspace.output.as_deref() == Some(connector),
            None => true,
        },
    }
}

fn focus(caller: &Caller, mode: PagerMode, id: u64) {
    match mode {
        PagerMode::Workspaces => caller.call::<FocusWorkspace>(FocusWorkspace {
            target: WorkspaceRef::Id { id },
        }),
        PagerMode::Windows => caller.call::<FocusWindow>(FocusWindow {
            target: WindowRef::Id { id },
        }),
    }
}

fn steps_windows(mode: PagerMode, horizontal: bool) -> bool {
    (mode == PagerMode::Windows) != horizontal
}

fn step(caller: &Caller, mode: PagerMode, horizontal: bool, forward: bool) {
    match steps_windows(mode, horizontal) {
        true => caller.call::<FocusWindow>(FocusWindow {
            target: match forward {
                true => WindowRef::Next,
                false => WindowRef::Prev,
            },
        }),
        false => caller.call::<FocusWorkspace>(FocusWorkspace {
            target: match forward {
                true => WorkspaceRef::Next,
                false => WorkspaceRef::Prev,
            },
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(id: u64, output: &str, focused: bool, active: bool) -> WorkspaceInfo {
        WorkspaceInfo {
            id,
            index: Some(id as u8),
            name: None,
            output: Some(output.to_owned()),
            active,
            focused,
            urgent: false,
            windows: 0,
        }
    }

    fn window(id: u64, focused: bool) -> WindowInfo {
        WindowInfo {
            id,
            title: Some("a terminal".to_owned()),
            app_id: Some("ghostty".to_owned()),
            workspace: Some(4),
            focused,
            floating: false,
            urgent: false,
            order: Some(1),
        }
    }

    fn labelled(label: &str, focused_label: &str) -> PagerConfig {
        PagerConfig {
            label: label.to_owned(),
            focused_label: focused_label.to_owned(),
            ..PagerConfig::default()
        }
    }

    #[test]
    fn a_window_numbers_itself_by_position_rather_than_by_its_address() {
        let slot = window_slot(
            &labelled("w{index}", "w{index}"),
            1,
            &window(94_388_234_684_768, false),
        );

        assert_eq!(
            slot.label, "w2",
            "a window has no compositor index, and under Hyprland its id is a memory address"
        );
        assert_eq!(slot.tooltip, "a terminal");
        assert_eq!(slot.id, 94_388_234_684_768);
    }

    #[test]
    fn the_focused_slot_reads_its_own_template() {
        let settings = labelled("{index}", "{name-or-index}");
        let mut named = workspace(2, "DP-1", false, false);
        named.name = Some("chat".to_owned());

        assert_eq!(workspace_slot(&settings, &named).label, "2");

        named.focused = true;
        assert_eq!(
            workspace_slot(&settings, &named).label,
            "chat",
            "focused-label exists so the current workspace can show its name while the rest \
             stay numbers"
        );
    }

    #[test]
    fn vertical_steps_whatever_the_strip_is_showing() {
        assert!(!steps_windows(PagerMode::Workspaces, false));
        assert!(steps_windows(PagerMode::Windows, false));
    }

    #[test]
    fn horizontal_steps_the_dimension_the_strip_is_not_showing() {
        assert!(steps_windows(PagerMode::Workspaces, true));
        assert!(!steps_windows(PagerMode::Windows, true));
    }

    #[test]
    fn scope_output_keeps_only_the_workspaces_on_this_panels_monitor() {
        let here = workspace(1, "DP-1", true, true);
        let there = workspace(9, "HDMI-A-1", false, true);

        assert!(in_scope(&here, PagerScope::Output, Some("DP-1")));
        assert!(!in_scope(&there, PagerScope::Output, Some("DP-1")));
    }

    #[test]
    fn scope_output_without_a_connector_shows_the_session_rather_than_nothing() {
        let there = workspace(9, "HDMI-A-1", false, true);

        assert!(
            in_scope(&there, PagerScope::Output, None),
            "an empty strip would look like a broken applet, not like a missing connector"
        );
    }

    #[test]
    fn scope_current_is_the_single_label_the_old_workspace_applet_was() {
        let focused = workspace(1, "DP-1", true, true);
        let current_elsewhere = workspace(9, "HDMI-A-1", false, true);

        assert!(in_scope(&focused, PagerScope::Current, Some("DP-1")));
        assert!(
            !in_scope(&current_elsewhere, PagerScope::Current, Some("DP-1")),
            "another output's current workspace is not the one the user is standing in"
        );
    }
}
