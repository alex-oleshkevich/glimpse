use gtk4::{CompositeTemplate, TemplateChild, gdk, glib, prelude::*, subclass::prelude::*};
use std::marker::PhantomData;

use crate::dots::Dots;
use crate::set_text;

const DOT_SCALE: f32 = 3.0;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate, glib::Properties)]
    #[properties(wrapper_type = super::EventRow)]
    #[template(resource = "/me/aresa/GlimpseShell/widgets/event_row.ui")]
    pub struct EventRow {
        #[template_child]
        pub time: TemplateChild<gtk4::Label>,

        #[property(name = "when", get = Self::when, set = Self::set_when, nullable)]
        when_text: PhantomData<Option<String>>,
    }

    impl EventRow {
        fn when(&self) -> Option<String> {
            self.time
                .get_visible()
                .then(|| self.time.text().to_string())
        }

        fn set_when(&self, when: Option<String>) {
            set_text(&self.time, when.as_deref());
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for EventRow {
        const NAME: &'static str = "EventRow";
        type Type = super::EventRow;
        type ParentType = crate::Row;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for EventRow {
        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for EventRow {}
    impl ButtonImpl for EventRow {}
    impl crate::row::RowImpl for EventRow {}
}

glib::wrapper! {
    pub struct EventRow(ObjectSubclass<imp::EventRow>)
        @extends crate::Row, gtk4::Button, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Actionable, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for EventRow {
    fn default() -> Self {
        Self::new()
    }
}

impl EventRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_color(&self, color: Option<gdk::RGBA>, reserve: bool) {
        let row: &crate::Row = self.upcast_ref();
        if !reserve {
            row.clear_lead();
            return;
        }
        let dots = self.dots();
        dots.set_colors(color.as_slice());
        row.set_lead(&dots);
    }

    fn dots(&self) -> Dots {
        let row: &crate::Row = self.upcast_ref();
        if let Some(dots) = row.lead().and_downcast::<Dots>() {
            return dots;
        }
        let dots = Dots::new();
        dots.add_css_class("event-list__dot");
        dots.set_max(1);
        dots.set_size(crate::dots::SIZE * DOT_SCALE);
        dots.set_valign(gtk4::Align::Center);
        dots
    }
}
