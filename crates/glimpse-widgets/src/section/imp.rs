use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, accessible, glib, prelude::*,
    subclass::prelude::*,
};
use std::cell::Cell;
use std::marker::PhantomData;

use crate::set_text;

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::Section)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/section.ui")]
pub struct Section {
    #[template_child]
    pub header: TemplateChild<gtk4::Box>,
    #[template_child]
    pub title: TemplateChild<gtk4::Label>,
    #[template_child]
    pub count: TemplateChild<gtk4::Label>,
    #[template_child]
    pub content: TemplateChild<gtk4::Box>,
    #[template_child]
    pub placeholder: TemplateChild<gtk4::Box>,

    #[property(name = "title", get = Self::title, set = Self::set_title, nullable)]
    title_text: PhantomData<Option<String>>,
    #[property(name = "count", get = Self::count, set = Self::set_count, nullable)]
    count_text: PhantomData<Option<String>>,
    #[property(name = "empty", get = Self::empty, set = Self::set_empty)]
    empty: Cell<bool>,
}

impl Section {
    fn title(&self) -> Option<String> {
        let title = self.title.text();
        (!title.is_empty()).then(|| title.to_string())
    }

    fn set_title(&self, title: Option<String>) {
        set_text(&self.title, title.as_deref());
        self.header.set_visible(!self.title.text().is_empty());
        self.obj()
            .update_property(&[accessible::Property::Label(self.title.text().as_str())]);
    }

    fn count(&self) -> Option<String> {
        let count = self.count.text();
        (!count.is_empty()).then(|| count.to_string())
    }

    fn set_count(&self, count: Option<String>) {
        set_text(&self.count, count.as_deref());
        self.sync_count();
    }

    fn sync_count(&self) {
        self.count
            .set_visible(!self.count.text().is_empty() && !self.empty());
    }

    fn empty(&self) -> bool {
        self.empty.get()
    }

    fn set_empty(&self, empty: bool) {
        if self.empty.replace(empty) == empty {
            return;
        }
        self.content.set_visible(!empty);
        self.placeholder.set_visible(empty);
        self.sync_count();
    }
}

#[glib::object_subclass]
impl ObjectSubclass for Section {
    const NAME: &'static str = "Section";
    type Type = super::Section;
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
impl ObjectImpl for Section {
    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for Section {}

impl BuildableImpl for Section {
    fn add_child(&self, builder: &gtk4::Builder, child: &glib::Object, kind: Option<&str>) {
        let own_template = self.placeholder.try_get().is_none();
        let section = self.obj();
        match (kind, child.downcast_ref::<gtk4::Widget>()) {
            _ if own_template => self.parent_add_child(builder, child, kind),
            (Some("placeholder"), Some(widget)) => section.set_placeholder(Some(widget)),
            (None, Some(widget)) => section.set_content(Some(widget)),
            _ => self.parent_add_child(builder, child, kind),
        }
    }
}
