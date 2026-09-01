use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};
use std::marker::PhantomData;

use crate::{set_css_class, set_text};

const NOW: &str = "forecast__now";

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate, glib::Properties)]
    #[properties(wrapper_type = super::ForecastHour)]
    #[template(resource = "/me/aresa/GlimpseShell/widgets/forecast_hour.ui")]
    pub struct ForecastHour {
        #[template_child]
        pub time: TemplateChild<gtk4::Label>,
        #[template_child]
        pub icon: TemplateChild<gtk4::Image>,
        #[template_child]
        pub temperature: TemplateChild<gtk4::Label>,

        #[property(name = "label", get = Self::label, set = Self::set_label, nullable)]
        label_text: PhantomData<Option<String>>,
        #[property(name = "icon-name", get = Self::icon_name, set = Self::set_icon_name, nullable)]
        icon_name: PhantomData<Option<String>>,
        #[property(name = "temperature", get = Self::temperature, set = Self::set_temperature, nullable)]
        temperature_text: PhantomData<Option<String>>,
        #[property(name = "now", get = Self::now, set = Self::set_now)]
        now: PhantomData<bool>,
    }

    impl ForecastHour {
        fn label(&self) -> Option<String> {
            self.time
                .get_visible()
                .then(|| self.time.text().to_string())
        }

        fn set_label(&self, label: Option<String>) {
            set_text(&self.time, label.as_deref());
        }

        fn icon_name(&self) -> Option<String> {
            self.icon.icon_name().map(|name| name.to_string())
        }

        fn set_icon_name(&self, name: Option<String>) {
            if self.icon_name() == name {
                return;
            }
            self.icon.set_icon_name(name.as_deref());
            self.icon.set_visible(name.is_some());
        }

        fn temperature(&self) -> Option<String> {
            self.temperature
                .get_visible()
                .then(|| self.temperature.text().to_string())
        }

        fn set_temperature(&self, temperature: Option<String>) {
            set_text(&self.temperature, temperature.as_deref());
        }

        fn now(&self) -> bool {
            self.time.has_css_class(NOW)
        }

        fn set_now(&self, now: bool) {
            set_css_class(&*self.time, NOW, now);
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ForecastHour {
        const NAME: &'static str = "ForecastHour";
        type Type = super::ForecastHour;
        type ParentType = gtk4::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.set_accessible_role(AccessibleRole::Group);
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for ForecastHour {
        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for ForecastHour {}
}

glib::wrapper! {
    pub struct ForecastHour(ObjectSubclass<imp::ForecastHour>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for ForecastHour {
    fn default() -> Self {
        Self::new()
    }
}

impl ForecastHour {
    pub fn new() -> Self {
        glib::Object::new()
    }
}
