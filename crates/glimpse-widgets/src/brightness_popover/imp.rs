use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{Fader, Hero, PopoverShell, Readout, Row, Section, Source, SourceList, SwitchRow};

use super::{
    CHANGED, FOOTER_ACTIVATED, NIGHT_LIGHT_CHANGED, NIGHT_LIGHT_MOVED, NIGHT_LIGHT_TOGGLED,
    TEMPERATURE_MAX, TEMPERATURE_MIN,
};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct NightLight {
    pub enabled: bool,
    pub temperature: u32,
}

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/brightness_popover.ui")]
pub struct BrightnessPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub hero: TemplateChild<Hero>,
    #[template_child]
    pub readout: TemplateChild<Readout>,
    #[template_child]
    pub primary: TemplateChild<Fader>,
    #[template_child]
    pub devices: TemplateChild<SourceList>,
    #[template_child]
    pub night_light: TemplateChild<Section>,
    #[template_child]
    pub enabled: TemplateChild<SwitchRow>,
    #[template_child]
    pub temperature: TemplateChild<Fader>,
    #[template_child]
    pub footer: TemplateChild<Row>,

    pub sources: RefCell<Vec<Source>>,
    pub night_light_state: RefCell<Option<NightLight>>,
    pub night_light_available: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for BrightnessPopover {
    const NAME: &'static str = "BrightnessPopover";
    type Type = super::BrightnessPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for BrightnessPopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder(CHANGED)
                    .param_types([String::static_type(), f64::static_type()])
                    .build(),
                glib::subclass::Signal::builder(NIGHT_LIGHT_TOGGLED)
                    .param_types([bool::static_type()])
                    .build(),
                glib::subclass::Signal::builder(NIGHT_LIGHT_CHANGED)
                    .param_types([f64::static_type()])
                    .build(),
                glib::subclass::Signal::builder(NIGHT_LIGHT_MOVED)
                    .param_types([f64::static_type()])
                    .build(),
                glib::subclass::Signal::builder(FOOTER_ACTIVATED).build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let popover = self.obj();

        self.temperature.set_maximum(TEMPERATURE_MAX);
        self.temperature.set_floor(TEMPERATURE_MIN);

        self.primary.connect_changed(glib::clone!(
            #[weak]
            popover,
            move |_, value| popover.report_primary_changed(value)
        ));

        self.devices.connect_changed(glib::clone!(
            #[weak]
            popover,
            move |_, key, value| popover.emit_by_name::<()>(CHANGED, &[&key, &value])
        ));

        self.enabled.connect_toggled(glib::clone!(
            #[weak]
            popover,
            move |_, on| popover.report_night_light_toggled(on)
        ));

        self.temperature.connect_changed(glib::clone!(
            #[weak]
            popover,
            move |_, value| popover.emit_by_name::<()>(NIGHT_LIGHT_CHANGED, &[&value])
        ));

        self.temperature.connect_moved(glib::clone!(
            #[weak]
            popover,
            move |_, value| popover.emit_by_name::<()>(NIGHT_LIGHT_MOVED, &[&value])
        ));

        self.footer.connect_clicked(glib::clone!(
            #[weak]
            popover,
            move |_| popover.emit_by_name::<()>(FOOTER_ACTIVATED, &[])
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for BrightnessPopover {}
