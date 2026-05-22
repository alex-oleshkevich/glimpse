mod imp;

use glib::closure_local;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::widgets::pager::{PagerAppearance, PagerItemView};

glib::wrapper! {
    pub struct PagerItem(ObjectSubclass<imp::PagerItem>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget,
                    gtk4::Orientable;
}

impl PagerItem {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_view(&self, view: &PagerItemView) {
        let imp = self.imp();
        set_class(self, "pager-dot", view.appearance == PagerAppearance::Dots);
        set_class(
            self,
            "pager-num",
            view.appearance == PagerAppearance::Numbers,
        );
        set_class(self, "active", view.active);
        set_class(self, "inactive", view.inactive);
        set_class(self, "occupied", view.occupied);
        set_class(self, "urgent", view.urgent);
        imp.label
            .set_visible(view.appearance == PagerAppearance::Numbers);
        imp.label.set_label(&view.label);
    }

    pub fn connect_activated(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            closure_local!(move |item: &Self| f(item)),
        )
    }
}

impl Default for PagerItem {
    fn default() -> Self {
        Self::new()
    }
}

fn set_class(widget: &impl gtk4::prelude::WidgetExt, class: &str, active: bool) {
    if active {
        widget.add_css_class(class);
    } else {
        widget.remove_css_class(class);
    }
}
