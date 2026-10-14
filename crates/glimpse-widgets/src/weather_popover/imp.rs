use std::cell::RefCell;

use gtk4::{AccessibleRole, CompositeTemplate, TemplateChild, glib, subclass::prelude::*};

use super::Page;
use crate::{ExpanderRow, ForecastList, ForecastStrip, Notice, PopoverShell, Readout, Section};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/weather_popover.ui")]
pub struct WeatherPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub reading: TemplateChild<Readout>,
    #[template_child]
    pub hourly: TemplateChild<Section>,
    #[template_child]
    pub hours: TemplateChild<ForecastStrip>,
    #[template_child]
    pub hourly_rule: TemplateChild<gtk4::Separator>,
    #[template_child]
    pub daily: TemplateChild<Section>,
    #[template_child]
    pub days: TemplateChild<ForecastList>,
    #[template_child]
    pub daily_rule: TemplateChild<gtk4::Separator>,
    #[template_child]
    pub nowcast: TemplateChild<Notice>,
    #[template_child]
    pub alerts: TemplateChild<gtk4::Box>,

    pub notices: RefCell<Vec<ExpanderRow>>,
    pub built: RefCell<Vec<Page>>,
}

#[glib::object_subclass]
impl ObjectSubclass for WeatherPopover {
    const NAME: &'static str = "WeatherPopover";
    type Type = super::WeatherPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for WeatherPopover {
    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for WeatherPopover {}
