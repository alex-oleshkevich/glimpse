use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{DriveId, RemovableError, RemovableHandle, RemovableState, VolumeId};

use glimpse_widgets::{IndicatorSpec, RemovablePopover};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{Applet, Ctx, Input, Report, spawn_reported};

use super::render;

pub struct Removable {
    removable: RemovableHandle,
    notifications: NotificationsProviderHandle,
    state: Rc<RefCell<RemovableState>>,
    tooltip_format: Option<String>,
    footer: Option<(String, Vec<String>)>,
    spec: Vec<IndicatorSpec>,
    volumes_cap: usize,
    expanded: Rc<Cell<bool>>,
    shown: glib::WeakRef<RemovablePopover>,
}

fn themed(name: &str) -> gio::Icon {
    gio::ThemedIcon::new(name).upcast()
}

impl Applet for Removable {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Removable(cfg) = &config.kind else {
            return;
        };
        self.volumes_cap = cfg.volumes;
        self.tooltip_format = config.common.tooltip_format.clone();
        self.footer = config
            .common
            .settings()
            .map(|(label, command)| (label.to_owned(), command.to_vec()));
        self.refresh();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => {
                self.state.replace(self.removable.snapshot());
            }
            Input::Tick | Input::Pointer(_) => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        self.expanded.set(false);

        let shown = RemovablePopover::new();

        shown.connect_activated({
            let state = Rc::clone(&self.state);
            let removable = self.removable.clone();
            let notifications = self.notifications.clone();
            move |_, id| match render::activation(&state.borrow(), id) {
                Some(render::Activation::Open(path)) => {
                    open(gio::File::for_path(&path).uri().to_string())
                }
                Some(render::Activation::Mount(id)) => {
                    let removable = removable.clone();
                    tell(
                        &notifications,
                        "removable.mount_volume",
                        gettext("Could not open that drive"),
                        async move { removable.mount(id).await },
                    );
                }
                None => {}
            }
        });

        shown.connect_eject({
            let removable = self.removable.clone();
            let notifications = self.notifications.clone();
            move |_, id| {
                let removable = removable.clone();
                let id = DriveId::new(id);
                tell(
                    &notifications,
                    "removable.eject_drive",
                    gettext("Could not eject that drive"),
                    async move { removable.eject(id).await },
                );
            }
        });

        shown.connect_unmount({
            let removable = self.removable.clone();
            let notifications = self.notifications.clone();
            move |_, id| {
                let removable = removable.clone();
                let id = VolumeId::new(id);
                tell(
                    &notifications,
                    "removable.unmount_volume",
                    gettext("Could not unmount that drive"),
                    async move { removable.unmount(id).await },
                );
            }
        });

        shown.connect_more({
            let expanded = Rc::clone(&self.expanded);
            let opener = seat.opener();
            move |_| {
                expanded.set(!expanded.get());
                opener.wake();
            }
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

fn open(uri: String) {
    relm4::spawn_local(async move {
        if let Err(error) =
            gio::AppInfo::launch_default_for_uri_future(&uri, gio::AppLaunchContext::NONE).await
        {
            tracing::warn!(uri, %error, "could not open that location");
        }
    });
}

fn tell<F, T>(
    notifications: &NotificationsProviderHandle,
    operation: &'static str,
    summary: String,
    future: F,
) where
    F: std::future::Future<Output = Result<T, RemovableError>> + Send + 'static,
    T: Send + 'static,
{
    let report = Report {
        notifications: notifications.clone(),
        app_name: gettext("Removable"),
        icon: render::ICON.to_owned(),
        summary,
    };
    spawn_reported(operation, report, wording, future);
}

fn wording(error: &RemovableError) -> Option<String> {
    error.failure().map(render::wording)
}

impl Removable {
    pub fn start(removable: RemovableHandle, notifications: NotificationsProviderHandle) -> Self {
        let state = removable.snapshot();
        Self {
            removable,
            notifications,
            state: Rc::new(RefCell::new(state)),
            tooltip_format: None,
            footer: None,
            spec: Vec::new(),
            volumes_cap: 6,
            expanded: Rc::new(Cell::new(false)),
            shown: glib::WeakRef::new(),
        }
    }

    fn refresh(&mut self) {
        self.spec = self.indicator().into_iter().collect();
        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn dress(&self, shown: &RemovablePopover) {
        let state = self.state.borrow();
        let devices = render::devices(&state, self.volumes_cap, self.expanded.get());
        shown.set_devices(&devices.drives);
        shown.set_overflow(devices.more.as_deref());
        shown.set_footer(self.footer.as_ref().map(|(label, _)| label.as_str()));
    }

    fn indicator(&self) -> Option<IndicatorSpec> {
        let state = self.state.borrow();
        if !render::chip(&state) {
            return None;
        }
        Some(IndicatorSpec {
            icon: Some(themed(render::ICON)),
            tooltip: Some(render::tooltip(&state, self.tooltip_format.as_deref())),
            ..Default::default()
        })
    }
}
