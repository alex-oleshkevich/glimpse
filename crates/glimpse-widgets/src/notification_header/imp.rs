use std::{cell::RefCell, marker::PhantomData, sync::OnceLock};

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, gio, glib, prelude::*, subclass::prelude::*,
};

use super::DISMISSED;
use crate::set_text;

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::NotificationHeader)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/notification_header.ui")]
pub struct NotificationHeader {
    #[template_child]
    pub row: TemplateChild<gtk4::Box>,
    #[template_child]
    pub app_icon: TemplateChild<gtk4::Image>,
    #[template_child]
    pub app_name: TemplateChild<gtk4::Label>,
    #[template_child]
    pub when: TemplateChild<gtk4::Label>,
    #[template_child]
    pub close: TemplateChild<gtk4::Button>,

    pub gicon: RefCell<Option<gio::Icon>>,

    #[property(name = "app-name", get = Self::app_name, set = Self::set_app_name, nullable)]
    app_name_text: PhantomData<Option<String>>,
    #[property(name = "when", get = Self::when, set = Self::set_when, nullable)]
    when_text: PhantomData<Option<String>>,
    #[property(name = "icon-name", get = Self::icon_name, set = Self::set_icon_name, nullable)]
    icon_name: PhantomData<Option<String>>,
}

impl NotificationHeader {
    fn app_name(&self) -> Option<String> {
        text(&self.app_name)
    }

    fn set_app_name(&self, name: Option<String>) {
        set_text(&self.app_name, name.as_deref());
    }

    fn when(&self) -> Option<String> {
        text(&self.when)
    }

    fn set_when(&self, when: Option<String>) {
        set_text(&self.when, when.as_deref());
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
            .set_app_icon(icon.as_ref().map(|icon| icon.upcast_ref::<gio::Icon>()));
    }
}

fn text(label: &TemplateChild<gtk4::Label>) -> Option<String> {
    (!label.text().is_empty()).then(|| label.text().to_string())
}

#[glib::object_subclass]
impl ObjectSubclass for NotificationHeader {
    const NAME: &'static str = "NotificationHeader";
    type Type = super::NotificationHeader;
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
impl ObjectImpl for NotificationHeader {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| vec![glib::subclass::Signal::builder(DISMISSED).build()])
    }

    fn constructed(&self) {
        self.parent_constructed();
        self.close.connect_clicked(glib::clone!(
            #[weak(rename_to = header)]
            self,
            move |_| header.obj().emit_by_name::<()>(DISMISSED, &[])
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for NotificationHeader {}
