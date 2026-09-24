use gtk4::{AccessibleRole, glib, prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::OnceLock;

use super::{Event, more_row};
use crate::{Expandable, Row};

#[derive(Debug)]
pub struct EventList {
    pub events: RefCell<Vec<Event>>,
    pub holders: RefCell<Vec<(String, Expandable)>>,
    pub cards: RefCell<HashMap<String, Event>>,
    pub earlier: Row,
    pub rows: gtk4::Box,
    pub more: Row,
    pub max_rows: Cell<u32>,
    pub show_earlier: Cell<bool>,
    pub show_all: Cell<bool>,
    pub activatable: Cell<bool>,
}

impl Default for EventList {
    fn default() -> Self {
        Self {
            events: RefCell::default(),
            holders: RefCell::default(),
            cards: RefCell::default(),
            earlier: more_row(),
            rows: gtk4::Box::new(gtk4::Orientation::Vertical, 0),
            more: more_row(),
            max_rows: Cell::default(),
            show_earlier: Cell::default(),
            show_all: Cell::default(),
            activatable: Cell::default(),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for EventList {
    const NAME: &'static str = "EventList";
    type Type = super::EventList;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BoxLayout>();
        klass.set_accessible_role(AccessibleRole::List);
    }
}

impl ObjectImpl for EventList {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("activated")
                    .param_types([u32::static_type()])
                    .build(),
                glib::subclass::Signal::builder("link-activated")
                    .param_types([String::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let list = self.obj();
        list.add_css_class("event-list");
        if let Some(layout) = list.layout_manager().and_downcast::<gtk4::BoxLayout>() {
            layout.set_orientation(gtk4::Orientation::Vertical);
        }
        self.earlier.set_parent(&*list);
        self.rows.set_parent(&*list);
        self.more.set_parent(&*list);
        self.earlier.connect_clicked(glib::clone!(
            #[weak]
            list,
            move |_| {
                list.imp().show_earlier.set(true);
                list.render();
            }
        ));
        self.more.connect_clicked(glib::clone!(
            #[weak]
            list,
            move |_| {
                list.imp().show_all.set(true);
                list.render();
            }
        ));

        list.set_has_tooltip(true);
        list.connect_query_tooltip(|list, _x, y, _keyboard, tooltip| match list.summary_at(y) {
            Some(summary) => {
                tooltip.set_text(Some(&summary));
                true
            }
            None => false,
        });
    }

    fn dispose(&self) {
        self.earlier.unparent();
        self.rows.unparent();
        self.more.unparent();
    }
}

impl WidgetImpl for EventList {}
