use gtk4::{AccessibleRole, glib, prelude::*, subclass::prelude::*};
use std::cell::RefCell;
use std::sync::OnceLock;

use super::{Player, PlayerRow};

#[derive(Debug, Default)]
pub struct PlayerList {
    pub players: RefCell<Vec<Player>>,
    pub rows: RefCell<Vec<PlayerRow>>,
}

#[glib::object_subclass]
impl ObjectSubclass for PlayerList {
    const NAME: &'static str = "PlayerList";
    type Type = super::PlayerList;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BoxLayout>();
        klass.set_accessible_role(AccessibleRole::List);
    }
}

impl ObjectImpl for PlayerList {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("activated")
                    .param_types([u32::static_type()])
                    .build(),
                glib::subclass::Signal::builder("toggled")
                    .param_types([u32::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let list = self.obj();
        list.add_css_class("player-list");
        if let Some(layout) = list.layout_manager().and_downcast::<gtk4::BoxLayout>() {
            layout.set_orientation(gtk4::Orientation::Vertical);
        }
    }

    fn dispose(&self) {
        for row in self.rows.borrow_mut().drain(..) {
            row.unparent();
        }
    }
}

impl WidgetImpl for PlayerList {}
