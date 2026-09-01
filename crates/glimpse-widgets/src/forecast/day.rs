use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
use std::marker::PhantomData;

use crate::{RangeBar, set_text};

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate, glib::Properties)]
    #[properties(wrapper_type = super::ForecastDay)]
    #[template(resource = "/me/aresa/GlimpseShell/widgets/forecast_day.ui")]
    pub struct ForecastDay {
        #[template_child]
        pub precipitation: TemplateChild<gtk4::Label>,
        #[template_child]
        pub low: TemplateChild<gtk4::Label>,
        #[template_child]
        pub bar: TemplateChild<gtk4::Box>,
        #[template_child]
        pub high: TemplateChild<gtk4::Label>,

        #[property(name = "precipitation", get = Self::precipitation, set = Self::set_precipitation, nullable)]
        precipitation_text: PhantomData<Option<String>>,
        #[property(name = "low", get = Self::low, set = Self::set_low, nullable)]
        low_text: PhantomData<Option<String>>,
        #[property(name = "high", get = Self::high, set = Self::set_high, nullable)]
        high_text: PhantomData<Option<String>>,
    }

    impl ForecastDay {
        fn precipitation(&self) -> Option<String> {
            self.precipitation
                .get_visible()
                .then(|| self.precipitation.text().to_string())
        }

        fn set_precipitation(&self, chance: Option<String>) {
            set_text(&self.precipitation, chance.as_deref());
        }

        fn low(&self) -> Option<String> {
            self.low.get_visible().then(|| self.low.text().to_string())
        }

        fn set_low(&self, low: Option<String>) {
            set_text(&self.low, low.as_deref());
        }

        fn high(&self) -> Option<String> {
            self.high
                .get_visible()
                .then(|| self.high.text().to_string())
        }

        fn set_high(&self, high: Option<String>) {
            set_text(&self.high, high.as_deref());
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ForecastDay {
        const NAME: &'static str = "ForecastDay";
        type Type = super::ForecastDay;
        type ParentType = crate::Row;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for ForecastDay {
        fn constructed(&self) {
            self.parent_constructed();
            self.bar.append(&RangeBar::new());
        }

        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for ForecastDay {}
    impl ButtonImpl for ForecastDay {}
    impl crate::row::RowImpl for ForecastDay {}
}

glib::wrapper! {
    pub struct ForecastDay(ObjectSubclass<imp::ForecastDay>)
        @extends crate::Row, gtk4::Button, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Actionable, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for ForecastDay {
    fn default() -> Self {
        Self::new()
    }
}

impl ForecastDay {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn bar(&self) -> RangeBar {
        self.imp()
            .bar
            .first_child()
            .and_downcast()
            .expect("range bar")
    }
}
