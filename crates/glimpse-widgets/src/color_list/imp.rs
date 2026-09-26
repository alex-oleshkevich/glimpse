use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use gtk4::{AccessibleRole, glib, prelude::*, subclass::prelude::*};

use super::Shade;
use crate::{Expandable, Row};

#[derive(Debug, Default)]
pub struct ColorList {
    pub shades: RefCell<Vec<Shade>>,
    pub holders: RefCell<Vec<(u64, Expandable)>>,
    pub rows: gtk4::Box,
    pub more: Row,
    pub show_all: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for ColorList {
    const NAME: &'static str = "ColorList";
    type Type = super::ColorList;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BoxLayout>();
        klass.set_accessible_role(AccessibleRole::List);
    }
}

impl ObjectImpl for ColorList {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("activated")
                    .param_types([u64::static_type()])
                    .build(),
                glib::subclass::Signal::builder("copied")
                    .param_types([u64::static_type(), String::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let list = self.obj();
        list.add_css_class("color-list");
        if let Some(layout) = list.layout_manager().and_downcast::<gtk4::BoxLayout>() {
            layout.set_orientation(gtk4::Orientation::Vertical);
        }
        self.rows.set_orientation(gtk4::Orientation::Vertical);
        self.rows.set_parent(&*list);
        self.more.set_lead_icon(Some("view-more-symbolic"));
        self.more.set_visible(false);
        self.more.set_parent(&*list);
        self.more.connect_clicked(glib::clone!(
            #[weak]
            list,
            move |_| {
                list.imp().show_all.set(true);
                list.render();
            }
        ));
    }

    fn dispose(&self) {
        self.holders.borrow_mut().clear();
        self.rows.unparent();
        self.more.unparent();
    }
}

impl WidgetImpl for ColorList {}
