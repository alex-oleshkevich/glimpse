use std::cell::{Cell, RefCell};
use std::rc::Rc;

use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_services::{PlacesHandle, PlacesState};

use glimpse_widgets::{IndicatorSpec, PlacesPopover};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{Applet, Ctx, Input};

use super::render;

pub struct Places {
    places: PlacesHandle,
    places_state: Rc<RefCell<PlacesState>>,
    tooltip_format: Option<String>,
    footer: Option<(String, Vec<String>)>,
    spec: Vec<IndicatorSpec>,
    bookmarks_cap: usize,
    expanded: Rc<Cell<bool>>,
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

        let shown = PlacesPopover::new();

        shown.connect_activated({
            let places_state = Rc::clone(&self.places_state);
            move |_, id| match render::activation(&places_state.borrow(), id) {
                Some(render::Activation::Open(path)) => {
                    open(gio::File::for_path(&path).uri().to_string())
                }
                Some(render::Activation::OpenUri(uri)) => open(uri.to_owned()),
                None => {}
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

impl Places {
    pub fn start(places: PlacesHandle) -> Self {
        let places_state = places.snapshot();
        Self {
            places,
            places_state: Rc::new(RefCell::new(places_state)),
            tooltip_format: None,
            footer: None,
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
        shown.set_footer(self.footer.as_ref().map(|(label, _)| label.as_str()));
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
