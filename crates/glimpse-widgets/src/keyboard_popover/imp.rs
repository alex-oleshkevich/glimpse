use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{PopoverShell, Row, Section};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Layout {
    pub name: String,
    pub code: String,
    pub active: bool,
}

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/keyboard_popover.ui")]
pub struct KeyboardPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub layouts: TemplateChild<Section>,
    #[template_child]
    pub rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub footer: TemplateChild<Row>,
    pub layouts_data: RefCell<Vec<Layout>>,
    pub widgets: RefCell<Vec<Row>>,
}

#[glib::object_subclass]
impl ObjectSubclass for KeyboardPopover {
    const NAME: &'static str = "KeyboardPopover";
    type Type = super::KeyboardPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for KeyboardPopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("activated")
                    .param_types([u32::static_type()])
                    .build(),
                glib::subclass::Signal::builder("footer-activated").build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let popover = self.obj();
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

impl WidgetImpl for KeyboardPopover {}
