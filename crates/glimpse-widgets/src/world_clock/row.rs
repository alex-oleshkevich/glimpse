use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
use std::marker::PhantomData;

use crate::set_text;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate, glib::Properties)]
    #[properties(wrapper_type = super::ClockRow)]
    #[template(resource = "/me/aresa/GlimpseShell/widgets/clock_row.ui")]
    pub struct ClockRow {
        #[template_child]
        pub phase: TemplateChild<gtk4::Image>,
        #[template_child]
        pub time: TemplateChild<gtk4::Label>,

        #[property(name = "time", get = Self::time, set = Self::set_time, nullable)]
        time_text: PhantomData<Option<String>>,
        #[property(name = "phase-icon", get = Self::phase_icon, set = Self::set_phase_icon, nullable)]
        phase_icon: PhantomData<Option<String>>,
    }

    impl ClockRow {
        fn time(&self) -> Option<String> {
            self.time
                .get_visible()
                .then(|| self.time.text().to_string())
        }

        fn set_time(&self, time: Option<String>) {
            set_text(&self.time, time.as_deref());
        }

        fn phase_icon(&self) -> Option<String> {
            self.phase.icon_name().map(|name| name.to_string())
        }

        fn set_phase_icon(&self, name: Option<String>) {
            if self.phase_icon() == name {
                return;
            }
            self.phase.set_icon_name(name.as_deref());
            self.phase.set_visible(name.is_some());
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ClockRow {
        const NAME: &'static str = "ClockRow";
        type Type = super::ClockRow;
        type ParentType = crate::Row;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for ClockRow {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_can_focus(false);
        }

        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for ClockRow {}
    impl ButtonImpl for ClockRow {}
    impl crate::row::RowImpl for ClockRow {}
}

glib::wrapper! {
    pub struct ClockRow(ObjectSubclass<imp::ClockRow>)
        @extends crate::Row, gtk4::Button, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Actionable, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for ClockRow {
    fn default() -> Self {
        Self::new()
    }
}

impl ClockRow {
    pub fn new() -> Self {
        glib::Object::new()
    }
}
