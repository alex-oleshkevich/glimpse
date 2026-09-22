use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{
    DriveId, PlacesHandle, PlacesState, RemovableError, RemovableHandle, RemovableState, VolumeId,
};

use glimpse_widgets::{IndicatorSpec, PlacesPopover};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{Applet, Ctx, Input, Report, spawn_reported};

use super::render;

pub struct Places {
    places: PlacesHandle,
    removable: RemovableHandle,
    notifications: NotificationsProviderHandle,
    places_state: Rc<RefCell<PlacesState>>,
    removable_state: Rc<RefCell<RemovableState>>,
    tooltip_format: Option<String>,
    footer: Option<(String, Vec<String>)>,
    spec: Vec<IndicatorSpec>,
    bookmarks_cap: usize,
    volumes_cap: usize,
    expanded: Rc<Cell<(bool, bool)>>,
    shown: glib::WeakRef<PlacesPopover>,
}

fn themed(name: &str) -> gio::Icon {
    gio::ThemedIcon::new(name).upcast()
}

impl Applet for Places {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Places(cfg) = &config.kind else {
            return;
        };
        self.bookmarks_cap = cfg.bookmarks;
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
                self.places_state.replace(self.places.snapshot());
                self.removable_state.replace(self.removable.snapshot());
            }
            Input::Tick | Input::Pointer(_) => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        self.expanded.set((false, false));

        let shown = PlacesPopover::new();

        shown.connect_activated({
            let places_state = Rc::clone(&self.places_state);
            let removable_state = Rc::clone(&self.removable_state);
            let removable = self.removable.clone();
            let notifications = self.notifications.clone();
            move |_, id| {
                let target =
                    render::activation(&places_state.borrow(), &removable_state.borrow(), id);
                match target {
                    Some(render::Activation::Open(path)) => {
                        open(gio::File::for_path(&path).uri().to_string())
                    }
                    Some(render::Activation::OpenUri(uri)) => open(uri.to_owned()),
                    Some(render::Activation::Mount(id)) => {
                        let removable = removable.clone();
                        tell(
                            &notifications,
                            "places.mount_volume",
                            gettext("Could not open that drive"),
                            async move { removable.mount(id).await },
                        );
                    }
                    None => {}
                }
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
                    "places.eject_drive",
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
                    "places.unmount_volume",
                    gettext("Could not unmount that drive"),
                    async move { removable.unmount(id).await },
                );
            }
        });

        shown.connect_more({
            let expanded = Rc::clone(&self.expanded);
            let opener = seat.opener();
            move |_, place| {
                let (bookmarks, devices) = expanded.get();
                match place {
                    "bookmarks" => expanded.set((!bookmarks, devices)),
                    "devices" => expanded.set((bookmarks, !devices)),
                    _ => return,
                }
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
        app_name: gettext("Places"),
        icon: render::ICON.to_owned(),
        summary,
    };
    spawn_reported(operation, report, wording, future);
}

fn wording(error: &RemovableError) -> Option<String> {
    error.failure().map(render::wording)
}

impl Places {
    pub fn start(
        places: PlacesHandle,
        removable: RemovableHandle,
        notifications: NotificationsProviderHandle,
    ) -> Self {
        let places_state = places.snapshot();
        let removable_state = removable.snapshot();
        Self {
            places,
            removable,
            notifications,
            places_state: Rc::new(RefCell::new(places_state)),
            removable_state: Rc::new(RefCell::new(removable_state)),
            tooltip_format: None,
            footer: None,
            spec: Vec::new(),
            bookmarks_cap: 8,
            volumes_cap: 6,
            expanded: Rc::new(Cell::new((false, false))),
            shown: glib::WeakRef::new(),
        }
    }

    fn refresh(&mut self) {
        self.spec = self.indicator().into_iter().collect();
        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn dress(&self, shown: &PlacesPopover) {
        let places_state = self.places_state.borrow();
        let removable_state = self.removable_state.borrow();

        shown.set_places(&render::places(&places_state));

        let bookmarks = render::bookmarks(&places_state, self.bookmarks_cap, self.expanded.get().0);
        shown.set_bookmarks(&bookmarks.entries);

        shown.set_network(&render::network(&places_state));

        let devices = render::devices(&removable_state, self.volumes_cap, self.expanded.get().1);
        shown.set_devices(&devices.drives);

        shown.set_overflow(bookmarks.more.as_deref(), devices.more.as_deref());
        shown.set_trash(render::trash(&places_state));
        shown.set_footer(self.footer.as_ref().map(|(label, _)| label.as_str()));
    }

    fn indicator(&self) -> Option<IndicatorSpec> {
        let places_state = self.places_state.borrow();
        let removable_state = self.removable_state.borrow();
        if !render::chip(&places_state, &removable_state) {
            return None;
        }
        Some(IndicatorSpec {
            icon: Some(themed(render::ICON)),
            tooltip: Some(render::tooltip(
                &places_state,
                &removable_state,
                self.tooltip_format.as_deref(),
            )),
            ..Default::default()
        })
    }
}
