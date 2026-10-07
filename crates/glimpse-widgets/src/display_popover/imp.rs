use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{
    ActionRow, ChoiceList, Display, DisplayList, ExpanderRow, PopoverShell, Readout, Row, Section,
    SliderRow, Source, SourceList, ToggleRow,
};

use super::{
    BLANKED, CHANGED, ENABLE_REQUESTED, FOOTER_ACTIVATED, NIGHT_LIGHT_CHANGED, NIGHT_LIGHT_MOVED,
    NIGHT_LIGHT_SCHEDULED, NIGHT_LIGHT_TOGGLED, TEMPERATURE_MAX, TEMPERATURE_MIN,
};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct NightLight {
    pub enabled: bool,
    pub temperature: u32,
    pub schedule: String,
    pub fixed_hours: bool,
    pub serving: bool,
}

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/display_popover.ui")]
pub struct DisplayPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub readout: TemplateChild<Readout>,
    #[template_child]
    pub brightness: TemplateChild<Section>,
    #[template_child]
    pub primary_name: TemplateChild<Row>,
    #[template_child]
    pub primary: TemplateChild<SliderRow>,
    #[template_child]
    pub devices: TemplateChild<SourceList>,
    #[template_child]
    pub night_light: TemplateChild<ExpanderRow>,
    #[template_child]
    pub night_facts: TemplateChild<gtk4::Box>,
    #[template_child]
    pub enabled: TemplateChild<ToggleRow>,
    #[template_child]
    pub temperature: TemplateChild<SliderRow>,
    #[template_child]
    pub schedules: TemplateChild<ChoiceList>,
    #[template_child]
    pub displays_section: TemplateChild<Section>,
    #[template_child]
    pub displays: TemplateChild<DisplayList>,
    #[template_child]
    pub blank: TemplateChild<ActionRow>,
    #[template_child]
    pub footer: TemplateChild<ActionRow>,

    pub sources: RefCell<Vec<Source>>,
    pub night_light_state: RefCell<Option<NightLight>>,
    pub night_light_available: Cell<bool>,
    pub entries: RefCell<Vec<Display>>,
    pub output_power: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for DisplayPopover {
    const NAME: &'static str = "DisplayPopover";
    type Type = super::DisplayPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        PopoverShell::static_type();
        Readout::static_type();
        Section::static_type();
        Row::static_type();
        SliderRow::static_type();
        SourceList::static_type();
        ExpanderRow::static_type();
        ToggleRow::static_type();
        ChoiceList::static_type();
        DisplayList::static_type();
        ActionRow::static_type();
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for DisplayPopover {
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
                glib::subclass::Signal::builder(NIGHT_LIGHT_SCHEDULED)
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder(ENABLE_REQUESTED)
                    .param_types([String::static_type(), bool::static_type()])
                    .build(),
                glib::subclass::Signal::builder(BLANKED).build(),
                glib::subclass::Signal::builder(FOOTER_ACTIVATED).build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let popover = self.obj();

        self.temperature.set_range(TEMPERATURE_MIN, TEMPERATURE_MAX);
        self.temperature.set_lead_icon("night-light-symbolic");

        self.primary.connect_changed(glib::clone!(
            #[weak]
            popover,
            move |_, value| popover.report_primary_changed(value)
        ));
        self.primary.connect_moved(glib::clone!(
            #[weak]
            popover,
            move |row, value| {
                let percent = crate::percent_of(value, SliderRow::max(row));
                popover.imp().readout.set_value(Some(percent.to_string()));
            }
        ));
        self.devices.connect_changed(glib::clone!(
            #[weak]
            popover,
            move |_, key, value| popover.emit_by_name::<()>(CHANGED, &[&key, &value])
        ));
        self.enabled.connect_toggled(glib::clone!(
            #[weak]
            popover,
            move |_, on| {
                popover.imp().temperature.set_sensitive(on);
                popover.emit_by_name::<()>(NIGHT_LIGHT_TOGGLED, &[&on]);
            }
        ));
        self.temperature.connect_changed(glib::clone!(
            #[weak]
            popover,
            move |_, value| {
                popover.show_temperature(value);
                popover.emit_by_name::<()>(NIGHT_LIGHT_CHANGED, &[&value]);
            }
        ));
        self.temperature.connect_moved(glib::clone!(
            #[weak]
            popover,
            move |_, value| {
                popover.show_temperature(value);
                popover.emit_by_name::<()>(NIGHT_LIGHT_MOVED, &[&value]);
            }
        ));
        self.schedules.connect_activated(glib::clone!(
            #[weak]
            popover,
            move |_, index| popover.report_night_light_scheduled(index)
        ));
        self.displays.connect_enable_requested(glib::clone!(
            #[weak]
            popover,
            move |_, connector, enabled| {
                popover.emit_by_name::<()>(ENABLE_REQUESTED, &[&connector, &enabled]);
            }
        ));
        self.blank.connect_activated(glib::clone!(
            #[weak]
            popover,
            move |_| popover.emit_by_name::<()>(BLANKED, &[])
        ));
        self.footer.connect_activated(glib::clone!(
            #[weak]
            popover,
            move |_| popover.emit_by_name::<()>(FOOTER_ACTIVATED, &[])
        ));

        popover.render_sources();
        popover.render_night_light();
        popover.render_displays();
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for DisplayPopover {}
