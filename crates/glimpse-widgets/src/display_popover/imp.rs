use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{Display, DisplayList, Hero, PopoverShell, Row, Section};

use super::{BLANKED, ENABLE_REQUESTED, FOOTER_ACTIVATED};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/display_popover.ui")]
pub struct DisplayPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub hero: TemplateChild<Hero>,
    #[template_child(id = "displays")]
    pub section: TemplateChild<Section>,
    #[template_child]
    pub devices: TemplateChild<DisplayList>,
    #[template_child]
    pub blank: TemplateChild<Row>,
    #[template_child]
    pub footer: TemplateChild<Row>,

    pub entries: RefCell<Vec<Display>>,
    pub output_power: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for DisplayPopover {
    const NAME: &'static str = "DisplayPopover";
    type Type = super::DisplayPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
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

        self.devices.connect_enable_requested(glib::clone!(
            #[weak]
            popover,
            move |_, connector, enabled| {
                popover.emit_by_name::<()>(ENABLE_REQUESTED, &[&connector, &enabled])
            }
        ));

        self.devices.connect_details_open_changed(glib::clone!(
            #[weak]
            popover,
            move |_, open| popover.set_details_open(open)
        ));

        self.blank.connect_clicked(glib::clone!(
            #[weak]
            popover,
            move |_| popover.emit_by_name::<()>(BLANKED, &[])
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

impl WidgetImpl for DisplayPopover {}
