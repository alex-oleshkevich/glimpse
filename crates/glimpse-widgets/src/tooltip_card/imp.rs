use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};
use std::cell::RefCell;
use std::marker::PhantomData;

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::TooltipCard)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/tooltip_card.ui")]
pub struct TooltipCard {
    #[template_child]
    pub icon: TemplateChild<gtk4::Image>,
    #[template_child]
    pub text: TemplateChild<gtk4::Box>,
    #[template_child]
    pub title: TemplateChild<gtk4::Label>,
    #[template_child]
    pub body: TemplateChild<gtk4::Label>,
    #[template_child]
    pub status: TemplateChild<gtk4::Label>,
    pub gicon: RefCell<Option<gio::Icon>>,

    #[property(name = "title", get = Self::title, set = Self::set_title, nullable)]
    title_text: PhantomData<Option<String>>,
    #[property(name = "body", get = Self::body, set = Self::set_body, nullable)]
    body_text: PhantomData<Option<String>>,
    #[property(name = "status", get = Self::status, set = Self::set_status, nullable)]
    status_text: PhantomData<Option<String>>,
    #[property(name = "icon-name", get = Self::icon_name, set = Self::set_icon_name, nullable)]
    icon_name: PhantomData<Option<String>>,
}

impl TooltipCard {
    fn title(&self) -> Option<String> {
        visible_text(&self.title)
    }

    fn set_title(&self, title: Option<String>) {
        let title = title.map(|title| super::truncate(&title, super::TITLE_MAX_CHARS));
        crate::set_text(&self.title, title.as_deref());
        self.obj().sync_visible();
    }

    fn body(&self) -> Option<String> {
        visible_text(&self.body)
    }

    fn set_body(&self, body: Option<String>) {
        let body = body.map(|body| super::clamp_body(&body));
        crate::set_text_capped(&self.body, body.as_deref(), super::BODY_MAX_CHARS);
        self.obj().sync_visible();
    }

    fn status(&self) -> Option<String> {
        visible_text(&self.status)
    }

    fn set_status(&self, status: Option<String>) {
        let status = status.map(|status| super::truncate(&status, super::TITLE_MAX_CHARS));
        crate::set_text(&self.status, status.as_deref());
        self.obj().sync_visible();
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
    label.get_visible().then(|| label.text().to_string())
}

#[glib::object_subclass]
impl ObjectSubclass for TooltipCard {
    const NAME: &'static str = "TooltipCard";
    type Type = super::TooltipCard;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Generic);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

#[glib::derived_properties]
impl ObjectImpl for TooltipCard {
    fn constructed(&self) {
        self.parent_constructed();
        self.obj().sync_visible();
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for TooltipCard {}
