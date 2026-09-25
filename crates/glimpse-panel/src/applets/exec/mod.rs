mod dom;
mod render;

use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    sync::Arc,
    sync::atomic::{AtomicU64, Ordering},
};

use gettextrs::gettext;
use glimpse_config::Position;
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{
    Edge, ExecBarRequest as BarRequest, ExecHandle, ExecUserEvent as UserEvent, Orientation,
    Placement, SessionActionsHandle, Tree, Zone,
};
use glimpse_widgets::{IndicatorSpec, PopoverShell};
use gtk4::{gio, prelude::*};

use crate::{
    app::AppInput,
    applet::{
        Applet, Ctx, Input, Pointer, Report,
        popover::{PopoverHandle, Seat},
        spawn_reported, wording,
    },
    components::panel,
};

static NEXT_SLOT: AtomicU64 = AtomicU64::new(1);

pub(crate) fn placement(
    position: Position,
    size: u32,
    output: Option<String>,
    zone: panel::Zone,
) -> Placement {
    Placement {
        output,
        position: match position {
            Position::Top => Edge::Top,
            Position::Bottom => Edge::Bottom,
            Position::Left => Edge::Left,
            Position::Right => Edge::Right,
        },
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
    actions: SessionActionsHandle,
    notifications: NotificationsProviderHandle,
    dialog: relm4::Sender<AppInput>,
    slot: u64,
    placement: Placement,
    tree: Arc<Tree>,
    generation: u64,
    title: String,
    icon: Option<String>,
    serial: u64,
    specs: Vec<IndicatorSpec>,
    icons: HashMap<String, gio::Icon>,
    seq: Rc<RefCell<HashMap<dom::Key, u64>>>,
    dom: Option<dom::Dom>,
}

impl Exec {
    pub fn start(
        name: &str,
        handle: ExecHandle,
        actions: SessionActionsHandle,
        notifications: NotificationsProviderHandle,
        dialog: relm4::Sender<AppInput>,
        placement: Placement,
    ) -> Self {
        let slot = NEXT_SLOT.fetch_add(1, Ordering::Relaxed);
        let report = Report {
            notifications: notifications.clone(),
            app_name: glimpse_utils::clean(name, 256),
            icon: "application-x-addon-symbolic".to_owned(),
            summary: gettext("External applet action failed."),
        };
        spawn_reported("exec.attach", report, command_wording, {
            let handle = handle.clone();
            let name = name.to_owned();
            let placement = placement.clone();
            async move { handle.attach(slot, &name, placement).await }
        });
        Self {
            handle,
            actions,
            notifications,
            dialog,
            slot,
            placement,
            tree: Arc::new(Tree::default()),
            generation: 0,
            title: name.to_owned(),
            icon: None,
            serial: 0,
            specs: Vec::new(),
            icons: HashMap::new(),
            seq: Rc::new(RefCell::new(HashMap::new())),
            dom: None,
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
        self.title = slot.title.clone();
        self.icon = slot.icon.clone();
        if !unchanged {
            self.tree = slot.tree.clone();
            self.specs = render::indicators(&self.tree, &mut |name| {
                render::cached_icon(&mut self.icons, name)
            })
            .into_iter()
            .map(|(_, spec)| spec)
            .collect();
        }
        if let Some(dom) = &mut self.dom
            && (!dom.is_alive()
                || ((!unchanged || generation_changed)
                    && !dom.reconcile(&self.tree, self.generation)))
        {
            self.dom = None;
        }
    }

    fn request(&self, request: &BarRequest, opener: &crate::applet::Opener) {
        match request {
            BarRequest::ClosePopover => opener.close_popover(),
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
            BarRequest::Session(_) => {
                let state = self.actions.snapshot();
                if let Some(input) = render::session_input(request, &state) {
                    let _ = self.dialog.send(input);
                }
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
            gesture: true,
        };
        send_event(self.handle.clone(), self.report(), event);
    }
}

fn command_wording(error: &glimpse_services::CommandError) -> Option<String> {
    wording(error, &gettext("External applet is not running."))
}

fn send_event(handle: ExecHandle, report: Report, event: UserEvent) {
    spawn_reported("exec.send_event", report, command_wording, async move {
        handle.send_event(event).await
    });
}

impl Drop for Exec {
    fn drop(&mut self) {
        let handle = self.handle.clone();
        let slot = self.slot;
        crate::applet::spawn_command("exec.detach", async move { handle.detach(slot).await });
    }
}

impl Applet for Exec {
    fn place(&mut self, placement: Placement) {
        if render::place_placement(&mut self.placement, placement.clone()) {
            let handle = self.handle.clone();
            let slot = self.slot;
            spawn_reported("exec.place", self.report(), command_wording, async move {
                handle.place(slot, placement).await
            });
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

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        render::first_child_of_kind(&self.tree, 0, |element| {
            matches!(element, glimpse_services::Element::Popover(_))
        })?;
        let shell = PopoverShell::new();
        let handle = self.handle.clone();
        let report = self.report();
        let slot = self.slot;
        shell.connect_map(move |_| {
            let handle = handle.clone();
            spawn_reported(
                "exec.popover",
                report.clone(),
                command_wording,
                async move { handle.popover(slot, true).await },
            );
        });
        let handle = self.handle.clone();
        let report = self.report();
        let opener = seat.opener();
        shell.connect_unmap(move |_| {
            let handle = handle.clone();
            spawn_reported(
                "exec.popover",
                report.clone(),
                command_wording,
                async move { handle.popover(slot, false).await },
            );
            opener.wake();
        });
        let dispatch = {
            let handle = self.handle.clone();
            let report = self.report();
            std::rc::Rc::new(move |event: UserEvent| {
                send_event(handle.clone(), report.clone(), event);
            })
        };
        let mut dom = dom::Dom::new(&shell, self.slot, dispatch, seat.opener(), self.seq.clone());
        dom.reconcile(&self.tree, self.generation);
        self.dom = Some(dom);
        Some(Box::new(shell))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn placement_carries_panel_zone_output_and_orientation() {
        assert_eq!(
            placement(Position::Left, 32, Some("DP-2".into()), panel::Zone::End),
            Placement {
                output: Some("DP-2".into()),
                position: Edge::Left,
                orientation: Orientation::Vertical,
                zone: Zone::Right,
                size: 32
            }
        );
    }
}
