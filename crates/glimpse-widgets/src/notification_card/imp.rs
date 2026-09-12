use std::{
    cell::{Cell, RefCell},
    marker::PhantomData,
    sync::OnceLock,
};

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, accessible, gdk, glib, prelude::*,
    subclass::prelude::*,
};

use super::{ACTION_INVOKED, ACTIVATED, DISMISSED};
use crate::{NotificationHeader, NotificationImageBody, NotificationTextBody};

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "NotificationUrgency")]
pub enum Urgency {
    #[default]
    Normal,
    Critical,
}

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::NotificationCard)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/notification_card.ui")]
pub struct NotificationCard {
    #[template_child]
    pub column: TemplateChild<gtk4::Box>,
    #[template_child]
    pub header: TemplateChild<NotificationHeader>,
    #[template_child]
    pub text: TemplateChild<NotificationTextBody>,
    #[template_child]
    pub image: TemplateChild<NotificationImageBody>,
    #[template_child]
    pub avatar: TemplateChild<gtk4::Picture>,
    #[template_child]
    pub progress: TemplateChild<gtk4::ProgressBar>,
    #[template_child]
    pub actions: TemplateChild<gtk4::Box>,

    pub avatar_source: RefCell<Option<gdk::Texture>>,
    pub shown: RefCell<Vec<super::Action>>,
    pub accessible_name: RefCell<String>,
    pub dismiss_name: RefCell<String>,
    pub activatable: Cell<bool>,

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

impl NotificationCard {
    fn summary(&self) -> Option<String> {
        self.text.title()
    }

    fn set_summary(&self, summary: Option<String>) {
        self.text.set_title(summary.as_deref());
        self.announce();
    }

    fn app_name(&self) -> Option<String> {
        self.header.app_name()
    }

    fn set_app_name(&self, name: Option<String>) {
        self.header.set_app_name(name.as_deref());
        self.announce();
    }

    fn when(&self) -> Option<String> {
        self.header.when()
    }

    fn set_when(&self, when: Option<String>) {
        self.header.set_when(when.as_deref());
        self.announce();
    }

    fn icon_name(&self) -> Option<String> {
        self.header.icon_name()
    }

    fn set_icon_name(&self, name: Option<String>) {
        self.header.set_icon_name(name.as_deref());
    }

    fn body(&self) -> Option<String> {
        self.text.body()
    }

    fn set_body(&self, body: Option<String>) {
        self.text.set_body(body.as_deref());
        self.announce();
    }

    fn body_markup(&self) -> Option<String> {
        self.text.body_markup()
    }

    fn set_body_markup(&self, markup: Option<String>) {
        self.text.set_body_markup(markup.as_deref());
        self.announce();
    }

    fn unread(&self) -> bool {
        self.unread.get()
    }

    fn set_unread(&self, unread: bool) {
        if self.unread.replace(unread) == unread {
            return;
        }
        self.announce();
    }

    fn urgency(&self) -> Urgency {
        self.urgency.get()
    }

    fn set_urgency(&self, urgency: Urgency) {
        self.urgency.set(urgency);
    }

    fn fraction(&self) -> f64 {
        self.fraction.get()
    }

    fn set_fraction(&self, fraction: f64) {
        if self.fraction.replace(fraction) == fraction {
            return;
        }
        self.progress.set_visible(fraction >= 0.0);
        if fraction >= 0.0 {
            self.progress.set_fraction(fraction);
        }
    }

    fn announce(&self) {
        let mut spoken = Vec::new();
        if self.unread.get() {
            spoken.push(gettextrs::gettext("Unread"));
        }
        spoken.extend(
            [
                self.header.app_name(),
                self.text.title(),
                self.text.body(),
                self.header.when(),
            ]
            .into_iter()
            .flatten()
            .map(|part| part.trim().to_owned())
            .filter(|part| !part.is_empty()),
        );
        let spoken = spoken.join(". ");
        self.obj()
            .update_property(&[accessible::Property::Label(&spoken)]);
        self.accessible_name.replace(spoken);

        let dismiss = match self
            .text
            .title()
            .map(|summary| summary.trim().to_owned())
            .filter(|summary| !summary.is_empty())
        {
            Some(summary) => {
                gettextrs::gettext("Dismiss {notification}").replace("{notification}", &summary)
            }
            None => gettextrs::gettext("Dismiss"),
        };
        self.header.set_dismiss_label(&dismiss);
        self.dismiss_name.replace(dismiss);
    }
}

#[glib::object_subclass]
impl ObjectSubclass for NotificationCard {
    const NAME: &'static str = "NotificationCard";
    type Type = super::NotificationCard;
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
impl ObjectImpl for NotificationCard {
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
        self.avatar.set_overflow(gtk4::Overflow::Hidden);
        self.header.connect_dismissed(glib::clone!(
            #[weak(rename_to = card)]
            self,
            move |_| card.obj().emit_by_name::<()>(DISMISSED, &[])
        ));

        let click = gtk4::GestureClick::new();
        click.set_button(gdk::BUTTON_PRIMARY);
        click.connect_released(glib::clone!(
            #[weak(rename_to = card)]
            self,
            move |gesture, _, _, _| {
                if card.activatable.get() {
                    gesture.set_state(gtk4::EventSequenceState::Claimed);
                    card.obj().emit_by_name::<()>(ACTIVATED, &[]);
                }
            }
        ));
        self.obj().add_controller(click);

        let key = gtk4::EventControllerKey::new();
        key.connect_key_pressed(glib::clone!(
            #[weak(rename_to = card)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, _| {
                if card.activatable.get()
                    && matches!(key, gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::space)
                {
                    card.obj().emit_by_name::<()>(ACTIVATED, &[]);
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            }
        ));
        self.obj().add_controller(key);
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for NotificationCard {}

impl BuildableImpl for NotificationCard {
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
