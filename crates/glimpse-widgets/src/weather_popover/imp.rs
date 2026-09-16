use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use super::Page;
use crate::{ForecastList, ForecastStrip, Hero, Notice, PopoverShell, Readout, Row, Section};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/weather_popover.ui")]
pub struct WeatherPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub hero: TemplateChild<Hero>,
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
    #[template_child]
    pub footer: TemplateChild<Row>,

    pub notices: RefCell<Vec<Notice>>,
    pub keys: RefCell<Vec<Option<String>>>,
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
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| vec![glib::subclass::Signal::builder("footer-activated").build()])
    }

    fn constructed(&self) {
        self.parent_constructed();
        let popover = self.obj();

        self.footer.connect_clicked(glib::clone!(
            #[weak]
            popover,
            move |_| popover.emit_by_name::<()>("footer-activated", &[])
        ));

        self.days.connect_activated(glib::clone!(
            #[weak]
            popover,
            move |_, index| popover.open(&super::day_page(index))
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for WeatherPopover {}
