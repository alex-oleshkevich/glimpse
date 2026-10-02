use std::cell::{Cell, RefCell};

use gtk4::{AccessibleRole, CompositeTemplate, glib, prelude::*, subclass::prelude::*};

const WIDE: &str = "applet-popover--wide";

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "PopoverSize")]
pub enum PopoverSize {
    #[default]
    Regular,
    Wide,
}

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::AppletPopover)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/applet_popover.ui")]
pub struct AppletPopover {
    #[property(get, set = Self::set_size, builder(PopoverSize::Regular).explicit_notify())]
    size: Cell<PopoverSize>,
    pub child: RefCell<Option<gtk4::Widget>>,
}

impl AppletPopover {
    fn set_size(&self, size: PopoverSize) {
        if self.size.get() == size {
            return;
        }
        self.size.set(size);
        let popover = self.obj();
        match size == PopoverSize::Wide {
            true => popover.add_css_class(WIDE),
            false => popover.remove_css_class(WIDE),
        }
        popover.notify_size();
    }
}

#[glib::object_subclass]
impl ObjectSubclass for AppletPopover {
    const NAME: &'static str = "AppletPopover";
    type Type = super::AppletPopover;
    type ParentType = gtk4::Widget;
    type Interfaces = (gtk4::Buildable,);

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

#[glib::derived_properties]
impl ObjectImpl for AppletPopover {
    fn dispose(&self) {
        if let Some(child) = self.child.borrow_mut().take() {
            child.unparent();
        }
        self.dispose_template();
    }
}

impl WidgetImpl for AppletPopover {}

impl BuildableImpl for AppletPopover {
    fn add_child(&self, builder: &gtk4::Builder, child: &glib::Object, kind: Option<&str>) {
        match (kind, child.downcast_ref::<gtk4::Widget>()) {
            (None, Some(widget)) => self.obj().set_child(Some(widget)),
            _ => self.parent_add_child(builder, child, kind),
        }
    }
}
