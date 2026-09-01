use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};
use std::cell::RefCell;
use std::marker::PhantomData;

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::Hero)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/hero.ui")]
pub struct Hero {
    #[template_child]
    pub icon: TemplateChild<gtk4::Image>,
    #[template_child]
    pub text: TemplateChild<gtk4::Box>,
    #[template_child]
    pub title: TemplateChild<gtk4::Label>,
    #[template_child]
    pub subtitle: TemplateChild<gtk4::Label>,
    #[template_child]
    pub slot: TemplateChild<gtk4::Box>,
    pub gicon: RefCell<Option<gio::Icon>>,

    #[property(name = "title", get = Self::title, set = Self::set_title, nullable)]
    title_text: PhantomData<Option<String>>,
    #[property(name = "subtitle", get = Self::subtitle, set = Self::set_subtitle, nullable)]
    subtitle_text: PhantomData<Option<String>>,
    #[property(name = "icon-name", get = Self::icon_name, set = Self::set_icon_name, nullable)]
    icon_name: PhantomData<Option<String>>,
}

impl Hero {
    fn title(&self) -> Option<String> {
        visible_text(&self.title)
    }

    fn set_title(&self, title: Option<String>) {
        super::set_text(&self.title, title.as_deref());
    }

    fn subtitle(&self) -> Option<String> {
        visible_text(&self.subtitle)
    }

    fn set_subtitle(&self, subtitle: Option<String>) {
        super::set_text(&self.subtitle, subtitle.as_deref());
    }

    fn icon_name(&self) -> Option<String> {
        self.gicon
            .borrow()
            .as_ref()
            .and_then(|icon| icon.downcast_ref::<gio::ThemedIcon>().cloned())
            .and_then(|icon| icon.names().first().map(|name| name.to_string()))
    }

    fn set_icon_name(&self, name: Option<String>) {
        let icon = name.map(|name| gio::ThemedIcon::new(&name));
        self.obj()
            .set_icon(icon.as_ref().map(|icon| icon.upcast_ref::<gio::Icon>()));
    }
}

fn visible_text(label: &TemplateChild<gtk4::Label>) -> Option<String> {
    label.is_visible().then(|| label.text().to_string())
}

#[glib::object_subclass]
impl ObjectSubclass for Hero {
    const NAME: &'static str = "Hero";
    type Type = super::Hero;
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
impl ObjectImpl for Hero {
    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for Hero {}

impl BuildableImpl for Hero {
    fn add_child(&self, builder: &gtk4::Builder, child: &glib::Object, kind: Option<&str>) {
        let own_template = self.slot.try_get().is_none();
        match (kind, child.downcast_ref::<gtk4::Widget>()) {
            _ if own_template => self.parent_add_child(builder, child, kind),
            (Some("slot"), Some(widget)) => self.obj().set_slot(widget),
            _ => self.parent_add_child(builder, child, kind),
        }
    }
}
