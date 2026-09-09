use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, accessible, glib, pango, prelude::*,
    subclass::prelude::*,
};
use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::sync::OnceLock;

use super::{ACTION_INVOKED, ACTIVATED, BODY_MAX_CHARS, DISMISSED};
use crate::{set_css_class, set_text, truncate};

const CRITICAL: &str = "notification--critical";
const UNREAD: &str = "notification--unread";

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "NotificationUrgency")]
pub enum Urgency {
    #[default]
    Normal,
    Critical,
}

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::NotificationItem)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/notification_item.ui")]
pub struct NotificationItem {
    #[template_child]
    pub overlay: TemplateChild<gtk4::Overlay>,
    #[template_child]
    pub activate: TemplateChild<gtk4::Button>,
    #[template_child]
    pub app_icon: TemplateChild<gtk4::Image>,
    #[template_child]
    pub summary: TemplateChild<gtk4::Label>,
    #[template_child]
    pub app_name: TemplateChild<gtk4::Label>,
    #[template_child]
    pub when: TemplateChild<gtk4::Label>,
    #[template_child]
    pub body: TemplateChild<gtk4::Label>,
    #[template_child]
    pub unread_dot: TemplateChild<gtk4::Box>,
    #[template_child]
    pub progress: TemplateChild<gtk4::ProgressBar>,
    #[template_child]
    pub picture: TemplateChild<gtk4::Picture>,
    #[template_child]
    pub actions: TemplateChild<gtk4::Box>,
    #[template_child]
    pub close: TemplateChild<gtk4::Button>,

    pub gicon: RefCell<Option<gio::Icon>>,
    pub markup: RefCell<Option<String>>,
    pub keys: RefCell<Vec<String>>,

    #[property(name = "summary", get = Self::summary, set = Self::set_summary, nullable)]
    summary_text: PhantomData<Option<String>>,
    #[property(name = "app-name", get = Self::app_name, set = Self::set_app_name, nullable)]
    app_name_text: PhantomData<Option<String>>,
    #[property(name = "when", get = Self::when, set = Self::set_when, nullable)]
    when_text: PhantomData<Option<String>>,
    #[property(name = "icon-name", get = Self::icon_name, set = Self::set_icon_name, nullable)]
    icon_name: PhantomData<Option<String>>,
    #[property(name = "body", get = Self::body, set = Self::set_body, nullable)]
    body_text: PhantomData<Option<String>>,
    #[property(name = "body-markup", get = Self::body_markup, set = Self::set_body_markup, nullable)]
    body_markup: PhantomData<Option<String>>,
    #[property(name = "unread", get = Self::unread, set = Self::set_unread)]
    unread: Cell<bool>,
    #[property(name = "urgency", get = Self::urgency, set = Self::set_urgency, builder(Urgency::Normal))]
    urgency: Cell<Urgency>,
    #[property(name = "progress", get = Self::fraction, set = Self::set_fraction, minimum = -1.0, maximum = 1.0, default = -1.0)]
    fraction: Cell<f64>,
}

impl NotificationItem {
    fn summary(&self) -> Option<String> {
        visible_text(&self.summary)
    }

    fn set_summary(&self, summary: Option<String>) {
        set_text(&self.summary, summary.as_deref());
        self.announce();
    }

    fn app_name(&self) -> Option<String> {
        visible_text(&self.app_name)
    }

    fn set_app_name(&self, name: Option<String>) {
        set_text(&self.app_name, name.as_deref());
    }

    fn when(&self) -> Option<String> {
        visible_text(&self.when)
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

    fn body(&self) -> Option<String> {
        visible_text(&self.body)
    }

    fn set_body(&self, body: Option<String>) {
        self.markup.replace(None);
        self.write_body(body.as_deref());
    }

    fn body_markup(&self) -> Option<String> {
        self.markup.borrow().clone()
    }

    /// The caller is expected to have run the text through `glimpse_utils::markup::sanitize_body`.
    /// This is the last gate rather than the first: markup Pango refuses leaves a `GtkLabel`
    /// showing nothing at all, so a body that does not parse is rendered as its own literal text
    /// instead of vanishing.
    fn set_body_markup(&self, markup: Option<String>) {
        if *self.markup.borrow() == markup {
            return;
        }
        self.markup.replace(markup.clone());

        let Some(markup) = markup else {
            self.write_body(None);
            return;
        };

        let capped = truncate(&markup, BODY_MAX_CHARS);
        if pango::parse_markup(&capped, '\0').is_err() {
            tracing::debug!("a notification body was refused by pango and reads as plain text");
            self.write_body(Some(&capped));
            return;
        }

        self.body.set_markup(&capped);
        self.body.set_visible(true);
        self.announce();
    }

    fn write_body(&self, text: Option<&str>) {
        let text = truncate(text.unwrap_or_default(), BODY_MAX_CHARS);
        self.body.set_text(&text);
        self.body.set_visible(!text.is_empty());
        self.announce();
    }

    fn unread(&self) -> bool {
        self.unread.get()
    }

    fn set_unread(&self, unread: bool) {
        if self.unread.replace(unread) == unread {
            return;
        }
        set_css_class(&*self.obj(), UNREAD, unread);
        self.unread_dot.set_visible(unread);
    }

    fn urgency(&self) -> Urgency {
        self.urgency.get()
    }

    fn set_urgency(&self, urgency: Urgency) {
        if self.urgency.replace(urgency) == urgency {
            return;
        }
        set_css_class(&*self.obj(), CRITICAL, urgency == Urgency::Critical);
    }

    fn fraction(&self) -> f64 {
        self.fraction.get()
    }

    /// Negative is "no progress at all", which a plain `f64` cannot say any other way.
    fn set_fraction(&self, fraction: f64) {
        if self.fraction.replace(fraction) == fraction {
            return;
        }
        self.progress.set_visible(fraction >= 0.0);
        if fraction >= 0.0 {
            self.progress.set_fraction(fraction);
        }
    }

    /// Both labels are `presentation`, so the activatable child is what carries the text a screen
    /// reader announces — and it announces it once rather than twice.
    fn announce(&self) {
        let spoken = [self.summary.text(), self.body.text()]
            .iter()
            .map(|part| part.trim().to_owned())
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(". ");

        self.activate
            .update_property(&[accessible::Property::Label(&spoken)]);
    }
}

fn visible_text(label: &TemplateChild<gtk4::Label>) -> Option<String> {
    label.get_visible().then(|| label.text().to_string())
}

#[glib::object_subclass]
impl ObjectSubclass for NotificationItem {
    const NAME: &'static str = "NotificationItem";
    type Type = super::NotificationItem;
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
impl ObjectImpl for NotificationItem {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder(ACTIVATED).build(),
                glib::subclass::Signal::builder(DISMISSED).build(),
                glib::subclass::Signal::builder(ACTION_INVOKED)
                    .param_types([String::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        self.fraction.set(-1.0);
        self.app_icon.set_overflow(gtk4::Overflow::Hidden);

        self.activate.connect_clicked(glib::clone!(
            #[weak(rename_to = item)]
            self,
            move |_| item.obj().emit_by_name::<()>(ACTIVATED, &[])
        ));
        self.close.connect_clicked(glib::clone!(
            #[weak(rename_to = item)]
            self,
            move |_| item.obj().emit_by_name::<()>(DISMISSED, &[])
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for NotificationItem {}

impl BuildableImpl for NotificationItem {
    fn add_child(&self, builder: &gtk4::Builder, child: &glib::Object, kind: Option<&str>) {
        let own_template = self.actions.try_get().is_none();
        match (kind, child.downcast_ref::<gtk4::Widget>()) {
            _ if own_template => self.parent_add_child(builder, child, kind),
            (Some("action"), Some(widget)) => {
                self.actions.append(widget);
                self.actions.set_visible(true);
            }
            _ => self.parent_add_child(builder, child, kind),
        }
    }
}
