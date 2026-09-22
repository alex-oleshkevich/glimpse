use std::cell::RefCell;
use std::rc::Rc;

use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{CompositorHandle, WorkspaceInfo};
use glimpse_widgets::{IndicatorSpec, WorkspaceNamePopover};
use gtk4::glib;

use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{Applet, Ctx, Input, Opener, Report, report_failure, wording};

use super::render;

type Pending = Rc<RefCell<Option<(u64, Option<String>)>>>;

pub struct WorkspaceName {
    compositor: CompositorHandle,
    notifications: NotificationsProviderHandle,
    output: Option<String>,
    workspaces: Vec<WorkspaceInfo>,
    current: Rc<RefCell<Option<WorkspaceInfo>>>,
    pending: Pending,
    tooltip_format: Option<String>,
    footer: Option<(String, Vec<String>)>,
    spec: Vec<IndicatorSpec>,
    shown: glib::WeakRef<WorkspaceNamePopover>,
}

impl Applet for WorkspaceName {
    fn configure(&mut self, ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::WorkspaceName {} = &config.kind else {
            return;
        };
        self.output = ctx.output().map(str::to_owned);
        self.tooltip_format = config.common.tooltip_format.clone();
        self.footer = config
            .common
            .settings()
            .map(|(label, command)| (label.to_owned(), command.to_vec()));
        self.refresh();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        if !matches!(input, Input::Woken) {
            return;
        }
        self.workspaces = snapshot(&self.compositor);
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = WorkspaceNamePopover::new();
        let opener = seat.opener();
        opener.typing(true);

        shown.connect_submitted({
            let opener = opener.clone();
            let current = self.current.clone();
            let pending = self.pending.clone();
            let compositor = self.compositor.clone();
            let notifications = self.notifications.clone();
            move |_, typed| {
                opener.close_popover();
                let Some(workspace) = current.borrow().clone() else {
                    return;
                };
                if let Some(name) = render::rename(workspace.name.as_deref(), &typed) {
                    rename(
                        &compositor,
                        &notifications,
                        &opener,
                        &pending,
                        workspace.id,
                        name,
                    );
                }
            }
        });

        shown.connect_cancelled({
            let opener = opener.clone();
            move |_| opener.close_popover()
        });

        if let Some((_, command)) = &self.footer {
            let command = command.clone();
            shown.connect_footer_activated(move |_| run(&command));
        }

        self.shown.set(Some(&shown));
        self.refresh();
        Some(Box::new(shown))
    }
}

impl WorkspaceName {
    pub fn start(compositor: CompositorHandle, notifications: NotificationsProviderHandle) -> Self {
        Self {
            workspaces: snapshot(&compositor),
            compositor,
            notifications,
            output: None,
            current: Rc::default(),
            pending: Rc::default(),
            tooltip_format: None,
            footer: None,
            spec: Vec::new(),
            shown: glib::WeakRef::new(),
        }
    }

    fn refresh(&mut self) {
        let mut current = render::current(&self.workspaces, self.output.as_deref()).cloned();
        if let (Some(workspace), Some((id, name))) = (current.as_mut(), &*self.pending.borrow())
            && workspace.id == *id
        {
            workspace.name = name.clone();
        }
        self.spec = current
            .iter()
            .map(|workspace| IndicatorSpec {
                label: Some(render::chip(workspace)),
                tooltip: Some(render::tooltip(workspace, self.tooltip_format.as_deref())),
                ..Default::default()
            })
            .collect();
        self.current.replace(current);
        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn dress(&self, shown: &WorkspaceNamePopover) {
        shown.set_footer(self.footer.as_ref().map(|(label, _)| label.as_str()));
        let current = self.current.borrow();
        let Some(workspace) = current.as_ref() else {
            shown.set_workspace("", "");
            shown.set_name("");
            return;
        };
        shown.set_workspace(&render::title(workspace), &render::subtitle(workspace));
        shown.set_name(workspace.name.as_deref().unwrap_or_default());
    }
}

fn snapshot(compositor: &CompositorHandle) -> Vec<WorkspaceInfo> {
    compositor
        .snapshot()
        .workspaces
        .map(|value| value.workspaces)
        .unwrap_or_default()
}

fn rename(
    compositor: &CompositorHandle,
    notifications: &NotificationsProviderHandle,
    opener: &Opener,
    pending: &Pending,
    id: u64,
    name: Option<String>,
) {
    let asked = (id, name.clone());
    pending.replace(Some(asked.clone()));
    opener.wake();

    let report = Report {
        notifications: notifications.clone(),
        app_name: gettext("Workspaces"),
        icon: render::ICON.to_owned(),
        summary: gettext("Could not rename the workspace"),
    };
    let unavailable = gettext("The compositor is unavailable.");
    let compositor = compositor.clone();
    let opener = opener.clone();
    let pending = pending.clone();
    relm4::spawn_local(async move {
        let outcome = compositor.rename_workspace(id, name).await;
        if pending.borrow().as_ref() == Some(&asked) {
            pending.replace(None);
            opener.wake();
        }
        let Err(error) = outcome else {
            return;
        };
        report_failure(
            "compositor.rename_workspace",
            report,
            wording(&error, &unavailable),
            error,
        )
        .await;
    });
}
