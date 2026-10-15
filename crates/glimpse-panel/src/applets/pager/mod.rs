use std::cell::RefCell;
use std::rc::Rc;

use gettextrs::{gettext, ngettext};
use glimpse_config::{
    Applet as AppletConfig, AppletKind, PagerConfig, PagerMode, PagerScope, PagerShape,
};
use glimpse_services::CompositorHandle;
use glimpse_services::{OutputInfo, WindowInfo, WindowRef, WorkspaceInfo, WorkspaceRef};
use glimpse_widgets::{
    Focus, Pager as Strip, Shape, Slot, Workspace, WorkspaceWindow, WorkspacesPopover,
};
use gtk4::glib;
use gtk4::prelude::*;

use crate::applet::popover::Seat;
use crate::applet::{Applet, Ctx, Input, spawn_command};
use crate::applets::output_name;
use crate::applets::workspace::{Facts, render, workspace_token};

pub struct Pager {
    strip: Strip,
    shown: glib::WeakRef<WorkspacesPopover>,
    settings: Rc<RefCell<PagerConfig>>,
    output: Option<String>,
    workspaces: Vec<WorkspaceInfo>,
    windows: Vec<WindowInfo>,
    outputs: Vec<OutputInfo>,
    compositor: CompositorHandle,
}

impl Applet for Pager {
    fn view(&mut self, ctx: &Ctx) -> Option<gtk4::Widget> {
        self.output = ctx.output().map(str::to_owned);

        self.strip.connect_pressed({
            let ctx = ctx.opener();
            move |_| ctx.toggle_popover()
        });

        self.strip.connect_stepped({
            let compositor = self.compositor.clone();
            let settings = self.settings.clone();
            move |_, horizontal, forward| {
                step(&compositor, settings.borrow().mode, horizontal, forward);
            }
        });

        Some(self.strip.clone().upcast())
    }

    fn orient(&mut self, orientation: gtk4::Orientation) {
        self.strip.set_orientation(orientation);
    }

    fn popover(&mut self, _seat: &Seat) -> Option<gtk4::Widget> {
        let shown = WorkspacesPopover::new();
        shown.set_workspaces(&self.rows());

        let compositor = self.compositor.clone();
        shown.connect_activated(move |id| {
            let compositor = compositor.clone();
            spawn_command("compositor.focus_workspace", async move {
                compositor.focus_workspace(WorkspaceRef::Id { id }).await
            });
        });

        let compositor = self.compositor.clone();
        shown.connect_window_activated(move |id| {
            let compositor = compositor.clone();
            spawn_command("compositor.focus_window", async move {
                compositor.focus_window(WindowRef::Id { id }).await
            });
        });

        self.shown.set(Some(&shown));
        Some(shown.upcast())
    }

    fn anchor(&self) -> Option<gtk4::Widget> {
        self.strip.anchor()
    }

    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Pager(settings) = &config.kind else {
            return;
        };
        self.settings.replace(settings.clone());
        self.render();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        if !matches!(input, Input::Woken) {
            return;
        }
        let state = self.compositor.snapshot();
        self.workspaces = state
            .workspaces
            .map(|value| value.workspaces)
            .unwrap_or_default();
        self.windows = state.windows.map(|value| value.windows).unwrap_or_default();
        self.outputs = state.outputs.map(|value| value.outputs).unwrap_or_default();
        self.render();
    }
}

impl Pager {
    pub fn start(compositor: CompositorHandle) -> Self {
        let state = compositor.snapshot();
        Self {
            strip: Strip::new(),
            shown: glib::WeakRef::new(),
            settings: Rc::new(RefCell::new(PagerConfig::default())),
            output: None,
            workspaces: state
                .workspaces
                .map(|value| value.workspaces)
                .unwrap_or_default(),
            windows: state.windows.map(|value| value.windows).unwrap_or_default(),
            outputs: state.outputs.map(|value| value.outputs).unwrap_or_default(),
            compositor,
        }
    }
    fn rows(&self) -> Vec<Workspace> {
        self.workspaces
            .iter()
            .map(|workspace| Workspace {
                id: workspace.id,
                label: workspace_token(workspace),
                detail: windows_label(workspace.windows),
                output: workspace.output.clone().unwrap_or_default(),
                display: workspace
                    .output
                    .as_deref()
                    .map(|connector| output_name(connector, &self.outputs))
                    .unwrap_or_default(),
                focused: workspace.focused,
                urgent: workspace.urgent,
                windows: self.windows_on(workspace.id),
            })
            .collect()
    }

    fn windows_on(&self, workspace: u64) -> Vec<WorkspaceWindow> {
        self.windows
            .iter()
            .filter(|window| window.workspace == Some(workspace))
            .map(|window| WorkspaceWindow {
                id: window.id,
                title: window
                    .title
                    .clone()
                    .or_else(|| window.app_id.clone())
                    .unwrap_or_default(),
                app_id: window.app_id.clone().unwrap_or_default(),
                focused: window.focused,
                urgent: window.urgent,
            })
            .collect()
    }

    fn render(&self) {
        let settings = self.settings.borrow();
        self.strip.set_shape(match settings.shape {
            PagerShape::Dots => Shape::Dots,
            PagerShape::Labels => Shape::Labels,
        });
        self.strip.set_slots(&self.slots(&settings));

        if let Some(shown) = self.shown.upgrade() {
            shown.set_workspaces(&self.rows());
        }
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
                let showing: Vec<&WorkspaceInfo> = scoped
                    .iter()
                    .filter(|workspace| workspace.active)
                    .copied()
                    .collect();
                let named = |id: u64| {
                    showing
                        .iter()
                        .find(|workspace| workspace.id == id)
                        .map(|workspace| workspace_token(workspace))
                };

                self.windows
                    .iter()
                    .filter(|window| {
                        window
                            .workspace
                            .is_some_and(|id| showing.iter().any(|it| it.id == id))
                    })
                    .enumerate()
                    .map(|(position, window)| {
                        let workspace = window.workspace.and_then(named);
                        window_slot(settings, position, window, workspace.as_deref())
                    })
                    .collect()
            }
        }
    }
}

fn windows_label(count: u32) -> String {
    match count {
        0 => gettext("Empty"),
        count => ngettext("{count} window", "{count} windows", count)
            .replace("{count}", &count.to_string()),
    }
}

fn template(settings: &PagerConfig, focused: bool, urgent: bool) -> &str {
    let chosen = match (urgent, focused) {
        (true, _) => settings.urgent_label.as_deref(),
        (false, true) => settings.focused_label.as_deref(),
        (false, false) => settings.unfocused_label.as_deref(),
    };
    chosen.unwrap_or(&settings.label)
}

fn workspace_slot(settings: &PagerConfig, workspace: &WorkspaceInfo) -> Slot {
    let named = workspace_token(workspace);
    let facts = Facts {
        index: workspace.index.map(u64::from),
        id: workspace.id,
        name: workspace.name.as_deref(),
        workspace: Some(&named),
    };

    Slot {
        id: workspace.id,
        label: render(
            template(settings, workspace.focused, workspace.urgent),
            &facts,
        ),
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

fn window_slot(
    settings: &PagerConfig,
    position: usize,
    window: &WindowInfo,
    workspace: Option<&str>,
) -> Slot {
    let facts = Facts {
        index: Some(position as u64 + 1),
        id: window.id,
        name: window.app_id.as_deref(),
        workspace,
    };

    Slot {
        id: window.id,
        label: render(template(settings, window.focused, window.urgent), &facts),
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

fn steps_windows(mode: PagerMode, horizontal: bool) -> bool {
    (mode == PagerMode::Windows) != horizontal
}

fn step(compositor: &CompositorHandle, mode: PagerMode, horizontal: bool, forward: bool) {
    let compositor = compositor.clone();
    match steps_windows(mode, horizontal) {
        true => spawn_command("compositor.focus_window", async move {
            compositor
                .focus_window(match forward {
                    true => WindowRef::Next,
                    false => WindowRef::Prev,
                })
                .await
        }),
        false => spawn_command("compositor.focus_workspace", async move {
            compositor
                .focus_workspace(match forward {
                    true => WorkspaceRef::Next,
                    false => WorkspaceRef::Prev,
                })
                .await
        }),
    }
}

#[cfg(test)]
mod tests;
