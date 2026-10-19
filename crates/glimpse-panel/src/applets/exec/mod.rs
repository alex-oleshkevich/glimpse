mod dom;
mod render;

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::Arc,
    sync::atomic::{AtomicU64, Ordering},
};

use gettextrs::gettext;
use glimpse_config::Position;
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{
    Edge, ExecBarRequest as BarRequest, ExecHandle, ExecStatus, ExecUserEvent as UserEvent,
    Orientation, Placement, Tree, Zone,
};
use glimpse_widgets::{ActionRow, IndicatorSpec, Notice, PopoverShell, Row, Severity};
use gtk4::{gio, glib, prelude::*};

use crate::{
    applet::{
        Applet, Ctx, Input, Pointer, Report, popover::Seat, spawn_command, spawn_reported, wording,
    },
    components::panel,
};

static NEXT_SLOT: AtomicU64 = AtomicU64::new(1);

fn next_slot() -> u64 {
    NEXT_SLOT.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn edge(position: Position) -> Edge {
    match position {
        Position::Top => Edge::Top,
        Position::Bottom => Edge::Bottom,
        Position::Left => Edge::Left,
        Position::Right => Edge::Right,
    }
}

pub(crate) fn placement(
    position: Position,
    size: u32,
    output: Option<String>,
    zone: panel::Zone,
) -> Placement {
    Placement {
        output,
        position: edge(position),
        orientation: match position {
            Position::Top | Position::Bottom => Orientation::Horizontal,
            Position::Left | Position::Right => Orientation::Vertical,
        },
        zone: match zone {
            panel::Zone::Start => Zone::Left,
            panel::Zone::Center => Zone::Center,
            panel::Zone::End => Zone::Right,
        },
        size,
    }
}

pub struct Exec {
    handle: ExecHandle,
    notifications: NotificationsProviderHandle,
    slot: u64,
    placement: Placement,
    tree: Arc<Tree>,
    status: ExecStatus,
    generation: u64,
    title: String,
    icon: Option<String>,
    serial: u64,
    specs: Vec<IndicatorSpec>,
    icons: HashMap<String, gio::Icon>,
    seq: Rc<RefCell<HashMap<dom::Key, u64>>>,
    dom: Option<dom::Dom>,
    failure_notice: glib::WeakRef<Notice>,
    restart_row: glib::WeakRef<Row>,
    restarting: Rc<Cell<bool>>,
}

impl Exec {
    pub fn start(
        name: &str,
        handle: ExecHandle,
        notifications: NotificationsProviderHandle,
        placement: Placement,
    ) -> Self {
        let slot = next_slot();
        let report = Report {
            notifications: notifications.clone(),
            app_name: glimpse_utils::clean(name, 256),
            icon: "application-x-addon-symbolic".to_owned(),
            summary: gettext("External applet action failed."),
        };
        spawn_reported(
            "exec.attach",
            report,
            command_wording,
            handle.attach(slot, name, placement.clone()),
        );
        Self {
            handle,
            notifications,
            slot,
            placement,
            tree: Arc::new(Tree::default()),
            status: ExecStatus::Starting,
            generation: 0,
            title: name.to_owned(),
            icon: None,
            serial: 0,
            specs: Vec::new(),
            icons: HashMap::new(),
            seq: Rc::new(RefCell::new(HashMap::new())),
            dom: None,
            failure_notice: glib::WeakRef::new(),
            restart_row: glib::WeakRef::new(),
            restarting: Rc::new(Cell::new(false)),
        }
    }

    fn report(&self) -> Report {
        Report {
            notifications: self.notifications.clone(),
            app_name: glimpse_utils::clean(&self.title, 256),
            icon: self
                .icon
                .clone()
                .unwrap_or_else(|| "application-x-addon-symbolic".to_owned()),
            summary: gettext("External applet action failed."),
        }
    }

    fn refresh(&mut self, opener: &crate::applet::Opener) {
        let snapshot = self.handle.snapshot();
        let Some(slot) = snapshot.slots.get(&self.slot) else {
            self.specs.clear();
            self.dom = None;
            return;
        };
        let (requests, unchanged) = render::requests_before_tree_check(
            &mut self.serial,
            &slot.requests,
            &self.tree,
            &slot.tree,
        );
        for request in &requests {
            self.request(request, opener);
        }
        let generation_changed = self.generation != slot.generation;
        if generation_changed {
            self.generation = slot.generation;
            self.icons.clear();
            self.seq.borrow_mut().clear();
        }
        let status_changed = self.status != slot.status;
        self.status = slot.status.clone();
        let title = if slot.title.is_empty() {
            &slot.applet
        } else {
            &slot.title
        };
        let title_changed = self.title != *title;
        self.title = title.clone();
        self.icon = slot.icon.clone();
        if !unchanged || status_changed || title_changed {
            self.tree = slot.tree.clone();
            self.specs = render::indicators(&self.tree, &mut |name| {
                render::cached_icon(&mut self.icons, name)
            })
            .into_iter()
            .map(|(_, spec)| spec)
            .collect();
            if render::failure(&self.status, !self.specs.is_empty()).is_some() {
                self.specs = vec![IndicatorSpec {
                    icon: Some(render::cached_icon(
                        &mut self.icons,
                        "dialog-error-symbolic",
                    )),
                    severity: Some(Severity::Error),
                    tooltip: Some(glimpse_utils::clean(&self.title, 256)),
                    ..Default::default()
                }];
            }
        }
        if let Some(notice) = self.failure_notice.upgrade() {
            if let Some(reason) = self.failure() {
                notice.set_subtitle(Some(glimpse_utils::clean(&reason, 256)));
                if let Some(row) = self.restart_row.upgrade() {
                    row.set_visible(matches!(
                        self.status,
                        ExecStatus::Failed(_) | ExecStatus::Ended
                    ));
                    row.set_busy(self.restarting.get());
                }
            } else if render::in_flight(&self.status, self.has_indicators()) {
                if let Some(row) = self.restart_row.upgrade() {
                    row.set_visible(true);
                    row.set_busy(true);
                }
            } else {
                opener.close_popover();
                self.failure_notice.set(None);
                self.restart_row.set(None);
            }
        }
        if let Some(dom) = &mut self.dom
            && (!dom.is_alive()
                || ((!unchanged || generation_changed || dom.needs_redress())
                    && !dom.reconcile(&self.tree, self.generation)))
        {
            self.dom = None;
        }
    }

    fn has_indicators(&self) -> bool {
        render::first_child_of_kind(&self.tree, 0, |element| {
            matches!(element, glimpse_services::Element::Indicator(_))
        })
        .is_some()
    }

    fn failure(&self) -> Option<String> {
        render::failure(&self.status, self.has_indicators())
    }

    fn request(&self, request: &BarRequest, opener: &crate::applet::Opener) {
        match request {
            BarRequest::ClosePopover => opener.close_popover(),
            BarRequest::OpenFile { uri } => {
                let file = gio::File::for_uri(uri);
                let report = self.report();
                relm4::spawn_local(async move {
                    let Some(path) = file.path() else {
                        return;
                    };
                    let checked = gio::spawn_blocking(move || render::openable(&path))
                        .await
                        .unwrap_or(Err("unreadable"));
                    let (path, content_type) = match checked {
                        Ok(checked) => checked,
                        Err(reason) => {
                            tracing::warn!(reason, "applet file not opened");
                            return;
                        }
                    };
                    let context =
                        gtk4::gdk::Display::default().map(|display| display.app_launch_context());
                    let launched = gio::AppInfo::default_for_type(&content_type, false)
                        .ok_or_else(|| {
                            glib::Error::new(gio::IOErrorEnum::NotSupported, &content_type)
                        })
                        .and_then(|app| app.launch(&[gio::File::for_path(path)], context.as_ref()));
                    if let Err(error) = launched {
                        crate::applet::report_failure(
                            "exec.open_file",
                            report,
                            Some(gettext("The file did not open.")),
                            error,
                        )
                        .await;
                    }
                });
            }
            BarRequest::OpenUri(uri) => {
                let uri = uri.clone();
                let report = self.report();
                relm4::spawn_local(async move {
                    let context =
                        gtk4::gdk::Display::default().map(|display| display.app_launch_context());
                    if let Err(error) =
                        gio::AppInfo::launch_default_for_uri_future(&uri, context.as_ref()).await
                    {
                        crate::applet::report_failure(
                            "exec.open_uri",
                            report,
                            Some(gettext("The link did not open.")),
                            error,
                        )
                        .await;
                    }
                });
            }
            BarRequest::Launch(id) => {
                let id = if id.ends_with(".desktop") {
                    id.clone()
                } else {
                    format!("{id}.desktop")
                };
                let report = self.report();
                relm4::spawn_local(async move {
                    let context =
                        gtk4::gdk::Display::default().map(|display| display.app_launch_context());
                    let launched = gio_unix::DesktopAppInfo::new(&id)
                        .ok_or_else(|| glib::Error::new(gio::IOErrorEnum::NotFound, &id))
                        .and_then(|app| app.launch(&[], context.as_ref()));
                    if let Err(error) = launched {
                        crate::applet::report_failure(
                            "exec.launch",
                            report,
                            Some(gettext("The application did not start.")),
                            error,
                        )
                        .await;
                    }
                });
            }
        }
    }

    fn send_pointer(&self, pointer: Pointer) {
        let Some((name, args, wanted)) = render::pointer(pointer) else {
            return;
        };
        let Some(id) = render::first_child_of_kind(
            &self.tree,
            0,
            |element| matches!(element, glimpse_services::Element::Indicator(p) if if wanted { p.on_press } else { p.on_scroll }),
        ) else {
            return;
        };
        let event = UserEvent {
            slot: self.slot,
            generation: self.generation,
            id,
            name: name.to_owned(),
            args,
            seq: None,
        };
        send_event(&self.handle, self.report(), event);
    }
}

fn command_wording(error: &glimpse_services::CommandError) -> Option<String> {
    wording(error, &gettext("External applet is not running."))
}

fn send_event(handle: &ExecHandle, report: Report, event: UserEvent) {
    spawn_reported(
        "exec.send_event",
        report,
        command_wording,
        handle.send_event(event),
    );
}

impl Drop for Exec {
    fn drop(&mut self) {
        spawn_command("exec.detach", self.handle.detach(self.slot));
    }
}

impl Applet for Exec {
    fn place(&mut self, placement: Placement) {
        if self.placement != placement {
            self.placement = placement.clone();
            spawn_command("exec.place", self.handle.place(self.slot, placement));
        }
    }

    fn handle(&mut self, ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => self.refresh(&ctx.opener()),
            Input::Pointer(pointer) => self.send_pointer(*pointer),
            Input::Tick => {}
        }
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.specs.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<gtk4::Widget> {
        if let Some(reason) = self.failure() {
            let shell = PopoverShell::new();
            shell.set_icon_name(self.icon.as_deref());
            shell.set_title(Some(glimpse_utils::clean(&self.title, 256)));
            let notice = Notice::new();
            notice.set_severity(Severity::Error);
            notice.set_title(Some(gettext("Not running")));
            notice.set_subtitle(Some(glimpse_utils::clean(&reason, 256)));
            shell.body().append(&notice);
            self.failure_notice.set(Some(&notice));
            let restart = ActionRow::new();
            let row = restart.upcast_ref::<Row>();
            row.set_title(Some(gettext("Restart")));
            row.set_visible(matches!(
                self.status,
                ExecStatus::Failed(_) | ExecStatus::Ended
            ));
            self.restart_row.set(Some(row));
            let handle = self.handle.clone();
            let slot = self.slot;
            let report = self.report();
            let restarting = Rc::clone(&self.restarting);
            let opener = seat.opener();
            restart.connect_activated(move |restart| {
                let row = restart.upcast_ref::<Row>();
                if restarting.get() || row.busy() {
                    return;
                }
                restarting.set(true);
                row.set_busy(true);
                let restarting = Rc::clone(&restarting);
                let opener = opener.clone();
                let handle = handle.clone();
                let report = report.clone();
                relm4::spawn_local(async move {
                    let outcome = handle.restart(slot).await;
                    restarting.set(false);
                    opener.wake();
                    if let Err(error) = outcome {
                        crate::applet::report_failure(
                            "exec.restart",
                            report,
                            command_wording(&error),
                            error,
                        )
                        .await;
                    }
                });
            });
            shell.body().append(&restart);
            return Some(shell.upcast());
        }
        render::first_child_of_kind(&self.tree, 0, |element| {
            matches!(element, glimpse_services::Element::Popover(_))
        })?;
        let shell = PopoverShell::new();
        let handle = self.handle.clone();
        let slot = self.slot;
        shell.connect_map(move |_| spawn_command("exec.popover", handle.popover(slot, true)));
        let handle = self.handle.clone();
        let opener = seat.opener();
        shell.connect_unmap(move |_| {
            spawn_command("exec.popover", handle.popover(slot, false));
            opener.wake();
        });
        let dispatch = {
            let handle = self.handle.clone();
            let report = self.report();
            Rc::new(move |event: UserEvent| send_event(&handle, report.clone(), event))
        };
        let mut dom = dom::Dom::new(&shell, self.slot, dispatch, seat.opener(), self.seq.clone());
        dom.reconcile(&self.tree, self.generation);
        self.dom = Some(dom);
        Some(shell.upcast())
    }
}

#[cfg(test)]
mod tests;
