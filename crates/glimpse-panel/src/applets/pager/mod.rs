mod label;

use std::cell::RefCell;
use std::rc::Rc;

use glimpse_config::{
    Applet as AppletConfig, AppletKind, PagerConfig, PagerMode, PagerScope, PagerShape,
};
use glimpse_contracts::{
    CompositorWindows, CompositorWorkspaces, FocusWindow, FocusWorkspace, Message, WindowInfo,
    WindowRef, WorkspaceInfo, WorkspaceRef,
};
use glimpse_widgets::{
    Focus, Pager as Strip, Shape, Slot, Workspace, WorkspaceWindow, WorkspacesPopover,
};
use gtk4::glib;
use gtk4::prelude::*;

use crate::applet::popover::{PopoverHandle, Seat};
use crate::applet::{Applet, Caller, Ctx, Input, payload};
use label::{Facts, render};

pub struct Pager {
    strip: Strip,
    shown: glib::WeakRef<WorkspacesPopover>,
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
            shown: glib::WeakRef::new(),
            settings: Rc::new(RefCell::new(PagerConfig::default())),
            output: None,
            workspaces: Vec::new(),
            windows: Vec::new(),
        }
    }

    fn view(&mut self, ctx: &Ctx) -> Option<gtk4::Widget> {
        self.output = ctx.output().map(str::to_owned);

        self.strip.connect_pressed({
            let ctx = ctx.opener();
            move |_| ctx.open_popover()
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

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = WorkspacesPopover::new();
        shown.set_workspaces(&self.rows());

        let caller = seat.caller();
        shown.connect_activated(move |id| {
            caller.call::<FocusWorkspace>(FocusWorkspace {
                target: WorkspaceRef::Id { id },
            });
        });

        let caller = seat.caller();
        shown.connect_window_activated(move |id| {
            caller.call::<FocusWindow>(FocusWindow {
                target: WindowRef::Id { id },
            });
        });

        self.shown.set(Some(&shown));
        Some(Box::new(Shown(shown)))
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
    fn rows(&self) -> Vec<Workspace> {
        self.workspaces
            .iter()
            .map(|workspace| Workspace {
                id: workspace.id,
                label: workspace_token(workspace),
                detail: match workspace.windows {
                    0 => "empty".to_owned(),
                    1 => "1 window".to_owned(),
                    many => format!("{many} windows"),
                },
                output: workspace.output.clone().unwrap_or_default(),
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

struct Shown(WorkspacesPopover);

impl PopoverHandle for Shown {
    fn root(&self) -> gtk4::Widget {
        self.0.clone().upcast()
    }
}

fn workspace_token(workspace: &WorkspaceInfo) -> String {
    match workspace.name.as_deref() {
        Some(name) if !name.is_empty() => name.to_owned(),
        _ => workspace
            .index
            .map_or_else(|| workspace.id.to_string(), |index| index.to_string()),
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
            focused_label: Some(focused_label.to_owned()),
            ..PagerConfig::default()
        }
    }

    #[test]
    fn a_window_numbers_itself_by_position_rather_than_by_its_address() {
        let slot = window_slot(
            &labelled("w{index}", "w{index}"),
            1,
            &window(94_388_234_684_768, false),
            Some("work"),
        );

        assert_eq!(
            slot.label, "w2",
            "a window has no compositor index, and under Hyprland its id is a memory address"
        );
        assert_eq!(slot.tooltip, "a terminal");
        assert_eq!(slot.id, 94_388_234_684_768);
    }

    #[test]
    fn an_unnamed_workspace_is_named_by_its_index() {
        let mut unnamed = workspace(7, "DP-1", false, false);
        unnamed.index = Some(4);
        unnamed.name = None;

        assert_eq!(workspace_token(&unnamed), "4");

        unnamed.name = Some(String::new());
        assert_eq!(
            workspace_token(&unnamed),
            "4",
            "a workspace renamed to nothing is unnamed, not named the empty string"
        );

        unnamed.name = Some("work".to_owned());
        assert_eq!(workspace_token(&unnamed), "work");
    }

    #[test]
    fn a_workspace_slot_renders_the_index_where_it_has_no_name() {
        let settings = PagerConfig {
            label: "{workspace-name}".to_owned(),
            ..PagerConfig::default()
        };
        let mut unnamed = workspace(7, "DP-1", false, false);
        unnamed.index = Some(4);

        assert_eq!(
            workspace_slot(&settings, &unnamed).label,
            "4",
            "an empty slot reads as a broken template, not as an unnamed workspace"
        );
    }

    #[test]
    fn a_window_slot_can_still_name_the_workspace_it_sits_on() {
        let slot = window_slot(
            &labelled("{workspace-name}", "{workspace-name}"),
            0,
            &window(94_388_234_684_768, false),
            Some("work"),
        );

        assert_eq!(
            slot.label, "work",
            "windows mode makes the name token the window's app id, so without this one there is \
             no way to show which workspace the strip is showing"
        );
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
             stay plain"
        );
    }

    #[test]
    fn each_state_reads_its_own_template_and_falls_back_to_the_plain_one() {
        let settings = PagerConfig {
            label: "{index}".to_owned(),
            focused_label: Some("[{index}]".to_owned()),
            unfocused_label: Some("-{index}-".to_owned()),
            urgent_label: Some("!{index}".to_owned()),
            ..PagerConfig::default()
        };

        assert_eq!(template(&settings, true, false), "[{index}]");
        assert_eq!(template(&settings, false, false), "-{index}-");
        assert_eq!(template(&settings, false, true), "!{index}");
        assert_eq!(
            template(&settings, true, true),
            "!{index}",
            "urgency outranks focus, because it is the one state the strip exists to surface"
        );

        let bare = PagerConfig {
            label: "{index}".to_owned(),
            ..PagerConfig::default()
        };
        for (focused, urgent) in [(true, true), (true, false), (false, true), (false, false)] {
            assert_eq!(
                template(&bare, focused, urgent),
                "{index}",
                "every state falls back to `label`, so setting one does not blank the others"
            );
        }
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
