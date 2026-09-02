mod imp;

use gtk4::{glib, prelude::*};

pub use imp::{Repeat, TransportAction};

glib::wrapper! {
    pub struct Transport(ObjectSubclass<imp::Transport>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for Transport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn connect_action<F: Fn(&Self, TransportAction) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "action",
            false,
            glib::closure_local!(move |transport: Self, action: TransportAction| f(
                &transport, action
            )),
        )
    }
}
