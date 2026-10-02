use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{PopoverShell, PopoverSize};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/widget_demo_popover.ui")]
pub struct WidgetDemoPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
}

#[glib::object_subclass]
impl ObjectSubclass for WidgetDemoPopover {
    const NAME: &'static str = "WidgetDemoPopover";
    type Type = super::WidgetDemoPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        PopoverShell::static_type();
        crate::Container::static_type();
        klass.bind_template();
        klass.bind_template_callbacks();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

#[gtk4::template_callbacks]
impl WidgetDemoPopover {
    #[template_callback]
    fn on_wide_toggled(&self, _: &glib::ParamSpec, switch: &gtk4::Switch) {
        self.shell.set_size(match switch.is_active() {
            true => PopoverSize::Wide,
            false => PopoverSize::Regular,
        });
    }
}

impl ObjectImpl for WidgetDemoPopover {
    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for WidgetDemoPopover {}
