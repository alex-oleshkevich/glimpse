use std::sync::OnceLock;

use glib::subclass::Signal;
use gtk4::{glib, prelude::*, subclass::prelude::*};

#[derive(Default)]
pub struct PagerItem {
    pub(super) label: gtk4::Label,
}

#[glib::object_subclass]
impl ObjectSubclass for PagerItem {
    const NAME: &'static str = "GlimpsePagerItem";
    type Type = super::PagerItem;
    type ParentType = gtk4::Box;
}

impl ObjectImpl for PagerItem {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.set_valign(gtk4::Align::Center);

        self.label.set_valign(gtk4::Align::Center);
        self.label.set_halign(gtk4::Align::Center);
        self.label.set_xalign(0.5);
        self.label.set_yalign(0.5);
        obj.append(&self.label);

        let gesture = gtk4::GestureClick::new();
        gesture.set_button(1);
        let weak = obj.downgrade();
        gesture.connect_released(move |gesture, _, _, _| {
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            if let Some(item) = weak.upgrade() {
                item.emit_by_name::<()>("activated", &[]);
            }
        });
        obj.add_controller(gesture);
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| vec![Signal::builder("activated").build()])
    }
}

impl WidgetImpl for PagerItem {}
impl BoxImpl for PagerItem {}
