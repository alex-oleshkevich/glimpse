use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{
    ChoiceList, Expandable, FactList, Hero, PopoverShell, Readout, Row, Section, SwitchRow,
};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/battery_popover.ui")]
pub struct BatteryPopover {
    pub devices: RefCell<Vec<super::Device>>,
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub hero: TemplateChild<Hero>,
    #[template_child]
    pub readout: TemplateChild<Readout>,
    #[template_child]
    pub profiles_section: TemplateChild<Section>,
    #[template_child]
    pub profiles: TemplateChild<ChoiceList>,
    #[template_child]
    pub devices_section: TemplateChild<Section>,
    #[template_child]
    pub devices_box: TemplateChild<gtk4::Box>,
    #[template_child]
    pub details: TemplateChild<Expandable>,
    #[template_child]
    pub details_row: TemplateChild<Row>,
    #[template_child]
    pub facts: TemplateChild<FactList>,
    #[template_child]
    pub charge_limit: TemplateChild<SwitchRow>,
    #[template_child]
    pub footer: TemplateChild<Row>,
}

#[glib::object_subclass]
impl ObjectSubclass for BatteryPopover {
    const NAME: &'static str = "BatteryPopover";
    type Type = super::BatteryPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        PopoverShell::static_type();
        Hero::static_type();
        Readout::static_type();
        Section::static_type();
        ChoiceList::static_type();
        Expandable::static_type();
        FactList::static_type();
        SwitchRow::static_type();
        Row::static_type();
        klass.set_layout_manager_type::<gtk4::BinLayout>();
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for BatteryPopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("profile-activated")
                    .param_types([u32::static_type()])
                    .build(),
                glib::subclass::Signal::builder("charge-limit-toggled")
                    .param_types([bool::static_type()])
                    .build(),
                glib::subclass::Signal::builder("footer-activated").build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let popover = self.obj();
        self.profiles.connect_activated(glib::clone!(
            #[weak]
            popover,
            move |_, index| popover.emit_by_name::<()>("profile-activated", &[&index])
        ));
        self.charge_limit.connect_toggled(glib::clone!(
            #[weak]
            popover,
            move |_, on| popover.emit_by_name::<()>("charge-limit-toggled", &[&on])
        ));
        self.footer.connect_clicked(glib::clone!(
            #[weak]
            popover,
            move |_| popover.emit_by_name::<()>("footer-activated", &[])
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for BatteryPopover {}
