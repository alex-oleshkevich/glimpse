use std::{cell::RefCell, marker::PhantomData};

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, pango, prelude::*, subclass::prelude::*,
};

use super::plain;
use crate::{set_text, truncate};
use glimpse_utils::markup::BODY_MAX_CHARS;

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::NotificationTextBody)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/notification_text_body.ui")]
pub struct NotificationTextBody {
    #[template_child]
    pub column: TemplateChild<gtk4::Box>,
    #[template_child]
    pub title: TemplateChild<gtk4::Label>,
    #[template_child]
    pub body: TemplateChild<gtk4::Label>,

    pub markup: RefCell<Option<String>>,
    #[property(name = "title", get = Self::title, set = Self::set_title, nullable)]
    title_text: PhantomData<Option<String>>,
    #[property(name = "body", get = Self::body, set = Self::set_body, nullable)]
    body_text: PhantomData<Option<String>>,
    #[property(name = "body-markup", get = Self::body_markup, set = Self::set_body_markup, nullable)]
    body_markup: PhantomData<Option<String>>,
}

impl NotificationTextBody {
    fn title(&self) -> Option<String> {
        text(&self.title)
    }

    fn set_title(&self, title: Option<String>) {
        set_text(&self.title, title.as_deref());
        self.sync();
    }

    fn body(&self) -> Option<String> {
        text(&self.body)
    }

    fn set_body(&self, body: Option<String>) {
        self.markup.replace(None);
        self.write_body(body.as_deref());
    }

    fn body_markup(&self) -> Option<String> {
        self.markup.borrow().clone()
    }

    fn set_body_markup(&self, markup: Option<String>) {
        let markup = markup.filter(|markup| !markup.trim().is_empty());
        if *self.markup.borrow() == markup {
            return;
        }
        self.markup.replace(markup.clone());
        let Some(markup) = markup else {
            self.write_body(None);
            return;
        };
        if pango::parse_markup(&markup, '\0').is_err() {
            tracing::debug!("a notification body was refused by pango and reads as plain text");
            self.write_body(Some(&plain(&markup)));
            return;
        }
        self.body.set_markup(&markup);
        self.body.set_visible(true);
        self.sync();
    }

    fn write_body(&self, text: Option<&str>) {
        let text = truncate(text.unwrap_or_default(), BODY_MAX_CHARS);
        if !self.body.uses_markup() && self.body.text() == text {
            return;
        }
        self.body.set_text(&text);
        self.body.set_visible(!text.is_empty());
        self.sync();
    }

    fn sync(&self) {
        self.title.set_visible(!self.title.text().is_empty());
        self.obj()
            .set_visible(self.title.get_visible() || self.body.get_visible());
    }
}

fn text(label: &TemplateChild<gtk4::Label>) -> Option<String> {
    (!label.text().is_empty()).then(|| label.text().to_string())
}

#[glib::object_subclass]
impl ObjectSubclass for NotificationTextBody {
    const NAME: &'static str = "NotificationTextBody";
    type Type = super::NotificationTextBody;
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
impl ObjectImpl for NotificationTextBody {
    fn constructed(&self) {
        self.parent_constructed();
        self.sync();
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for NotificationTextBody {}
