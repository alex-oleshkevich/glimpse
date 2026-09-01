use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};
use std::marker::PhantomData;

use crate::{set_css_class, set_text};

const ERROR: &str = "placeholder--error";

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::Placeholder)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/placeholder.ui")]
pub struct Placeholder {
    #[template_child]
    pub icon: TemplateChild<gtk4::Image>,
    #[template_child]
    pub title: TemplateChild<gtk4::Label>,
    #[template_child]
    pub description: TemplateChild<gtk4::Label>,

    #[property(name = "icon-name", get = Self::icon_name, set = Self::set_icon_name, nullable)]
    icon_name: PhantomData<Option<String>>,
    #[property(name = "title", get = Self::title, set = Self::set_title, nullable)]
    title_text: PhantomData<Option<String>>,
    #[property(name = "description", get = Self::description, set = Self::set_description, nullable)]
    description_text: PhantomData<Option<String>>,
    #[property(name = "error", get = Self::error, set = Self::set_error)]
    error: PhantomData<bool>,
}

impl Placeholder {
    fn icon_name(&self) -> Option<String> {
        self.icon.icon_name().map(|name| name.to_string())
    }

    fn set_icon_name(&self, name: Option<String>) {
        if self.icon_name() == name {
            return;
        }
        self.icon.set_icon_name(name.as_deref());
        self.icon.set_visible(name.is_some());
    }

    fn title(&self) -> Option<String> {
        self.title
            .get_visible()
            .then(|| self.title.text().to_string())
    }

    fn set_title(&self, title: Option<String>) {
        set_text(&self.title, title.as_deref());
    }

    fn description(&self) -> Option<String> {
        self.description
            .get_visible()
            .then(|| self.description.text().to_string())
    }

    fn set_description(&self, description: Option<String>) {
        set_text(&self.description, description.as_deref());
    }

    fn error(&self) -> bool {
        self.obj().has_css_class(ERROR)
    }

    fn set_error(&self, error: bool) {
        set_css_class(&*self.obj(), ERROR, error);
    }
}

#[glib::object_subclass]
impl ObjectSubclass for Placeholder {
    const NAME: &'static str = "Placeholder";
    type Type = super::Placeholder;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

#[glib::derived_properties]
impl ObjectImpl for Placeholder {
    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for Placeholder {}
