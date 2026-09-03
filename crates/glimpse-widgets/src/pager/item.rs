use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};
use std::cell::{Cell, RefCell};

use super::{Focus, Shape, Slot};
use crate::{TEXT_MAX_CHARS, set_css_class, truncate};

const LABEL_MAX_CHARS: usize = 12;

const HERE: &str = "pager-item--here";
const ELSEWHERE: &str = "pager-item--elsewhere";
const OCCUPIED: &str = "pager-item--occupied";
const URGENT: &str = "pager-item--urgent";

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/me/aresa/GlimpseShell/widgets/pager_item.ui")]
    pub struct PagerItem {
        #[template_child]
        pub label: TemplateChild<gtk4::Label>,
        pub slot: RefCell<Option<Slot>>,
        pub shape: Cell<Shape>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PagerItem {
        const NAME: &'static str = "PagerItem";
        type Type = super::PagerItem;
        type ParentType = gtk4::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.set_accessible_role(AccessibleRole::Generic);
        }

        fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
            object.init_template();
        }
    }

    impl ObjectImpl for PagerItem {
        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for PagerItem {}
}

glib::wrapper! {
    pub struct PagerItem(ObjectSubclass<imp::PagerItem>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for PagerItem {
    fn default() -> Self {
        Self::new()
    }
}

impl PagerItem {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_slot(&self, slot: &Slot, shape: Shape) {
        let imp = self.imp();
        if imp.slot.borrow().as_ref() == Some(slot) && imp.shape.get() == shape {
            return;
        }
        imp.slot.replace(Some(slot.clone()));
        imp.shape.set(shape);

        let label = truncate(&slot.label, LABEL_MAX_CHARS);
        imp.label
            .set_visible(shape == Shape::Labels && !label.is_empty());
        imp.label.set_text(&label);

        let described = match slot.tooltip.is_empty() {
            true => label,
            false => truncate(&slot.tooltip, TEXT_MAX_CHARS),
        };
        self.set_tooltip_text((!described.is_empty()).then_some(described.as_str()));
        self.update_property(&[gtk4::accessible::Property::Label(&described)]);

        set_css_class(self, HERE, slot.focus == Focus::Here);
        set_css_class(self, ELSEWHERE, slot.focus == Focus::Elsewhere);
        set_css_class(self, OCCUPIED, slot.occupied && slot.focus == Focus::None);
        set_css_class(self, URGENT, slot.urgent);
    }
}
