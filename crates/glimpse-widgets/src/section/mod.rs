mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{clear_children, fill_slot};

glib::wrapper! {
    pub struct Section(ObjectSubclass<imp::Section>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for Section {
    fn default() -> Self {
        Self::new()
    }
}

impl Section {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_visible(&self, visible: bool) {
        let imp = self.imp();
        if visible || !self.is_mapped() || crate::duration_ms() == 0 {
            imp.hiding.set(false);
            WidgetExt::set_visible(self, visible);
            return;
        }
        if !self.get_visible() || imp.hiding.replace(true) {
            return;
        }
        let Some(clock) = self.frame_clock() else {
            imp.hiding.set(false);
            WidgetExt::set_visible(self, false);
            return;
        };
        let handler = std::rc::Rc::new(std::cell::RefCell::new(None));
        let id = clock.connect_after_paint(glib::clone!(
            #[weak(rename_to = section)]
            self,
            #[strong]
            handler,
            move |clock| {
                if let Some(id) = handler.take() {
                    clock.disconnect(id);
                }
                if section.imp().hiding.replace(false) {
                    crate::reconcile::exit(section.upcast_ref());
                    WidgetExt::set_visible(&section, false);
                }
            }
        ));
        handler.replace(Some(id));
        self.queue_draw();
    }

    pub fn set_content(&self, content: Option<&impl IsA<gtk4::Widget>>) {
        fill(&self.imp().content, content);
    }

    pub fn set_placeholder(&self, placeholder: Option<&impl IsA<gtk4::Widget>>) {
        fill(&self.imp().placeholder, placeholder);
    }

    pub fn set_trail(&self, trail: Option<&impl IsA<gtk4::Widget>>) {
        let imp = self.imp();
        fill(&imp.trail, trail);
        imp.trail.set_visible(trail.is_some());
        imp.sync_header();
    }
}

fn fill(slot: &gtk4::Box, widget: Option<&impl IsA<gtk4::Widget>>) {
    match widget {
        Some(widget) => fill_slot(slot, widget),
        None => clear_children(slot),
    }
}
