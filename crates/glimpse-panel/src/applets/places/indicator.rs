use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{CommandError, PlacesHandle, PlacesState};

use glimpse_widgets::{IndicatorSpec, PlacesPopover};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use crate::applet::popover::Seat;
use crate::applet::themed;
use crate::applet::{Applet, Ctx, Input, Report, open_location, spawn_reported};

use super::render;

pub struct Places {
    places: PlacesHandle,
    notifications: NotificationsProviderHandle,
    places_state: Rc<RefCell<PlacesState>>,
    tooltip_format: Option<String>,
    spec: Vec<IndicatorSpec>,
    bookmarks_cap: usize,
    expanded: Rc<Cell<bool>>,
    shown: glib::WeakRef<PlacesPopover>,
}

impl Applet for Places {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Places(cfg) = &config.kind else {
            return;
        };
        self.bookmarks_cap = cfg.bookmarks;
        self.tooltip_format = config.kind.tooltip_format().map(str::to_owned);

        self.refresh();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => {
                self.places_state.replace(self.places.snapshot());
            }
            Input::Tick | Input::Pointer(_) => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<gtk4::Widget> {
        self.expanded.set(false);

        let shown = PlacesPopover::new();

        shown.connect_activated({
            let places_state = Rc::clone(&self.places_state);
            let report =
                Report::open_failure(self.notifications.clone(), gettext("Places"), render::ICON);
            let opener = seat.opener();
            move |_, id| {
                match render::activation(&places_state.borrow(), id) {
                    Some(render::Activation::Open(path)) => {
                        open_location(gio::File::for_path(&path).uri().to_string(), report.clone())
                    }
                    Some(render::Activation::OpenUri(uri)) => {
                        open_location(uri.to_owned(), report.clone())
                    }
                    None => return,
                }
                opener.close_popover();
            }
        });

        shown.connect_empty_trash({
            let places = self.places.clone();
            let report = Report {
                notifications: self.notifications.clone(),
                app_name: gettext("Places"),
                icon: render::ICON.to_owned(),
                summary: gettext("Could not empty the trash"),
            };
            move |_| {
                let places = places.clone();
                spawn_reported("places.empty_trash", report.clone(), wording, async move {
                    places.empty_trash().await
                });
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

        self.shown.set(Some(&shown));
        self.refresh();
        Some(shown.upcast())
    }
}

fn wording(error: &CommandError) -> Option<String> {
    match error {
        CommandError::LimitExceeded(_) => None,
        _ => Some(gettext("Some items could not be deleted.")),
    }
}

impl Places {
    pub fn start(places: PlacesHandle, notifications: NotificationsProviderHandle) -> Self {
        let places_state = places.snapshot();
        Self {
            places,
            notifications,
            places_state: Rc::new(RefCell::new(places_state)),
            tooltip_format: None,
            spec: Vec::new(),
            bookmarks_cap: 8,
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

    fn dress(&self, shown: &PlacesPopover) {
        let places_state = self.places_state.borrow();

        shown.set_places(&render::places(&places_state));

        let bookmarks = render::bookmarks(&places_state, self.bookmarks_cap, self.expanded.get());
        shown.set_bookmarks(&bookmarks.entries);

        shown.set_network(&render::network(&places_state));
        shown.set_overflow(bookmarks.more.as_deref());
        shown.set_trash(render::trash(&places_state));
    }

    fn indicator(&self) -> Option<IndicatorSpec> {
        let places_state = self.places_state.borrow();
        if !render::chip(&places_state) {
            return None;
        }
        Some(IndicatorSpec {
            icon: Some(themed(render::ICON)),
            tooltip: Some(render::tooltip(
                &places_state,
                self.tooltip_format.as_deref(),
            )),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests;
