mod imp;

use gtk4::{gdk, gio, glib, prelude::*, subclass::prelude::*};

pub use imp::Urgency;

use crate::{none_if_empty, truncate};

const LABEL_MAX_CHARS: usize = 32;
const ACTIONS_MAX: usize = 3;
const ACTIVATED: &str = "activated";
const DISMISSED: &str = "dismissed";
const ACTION_INVOKED: &str = "action-invoked";
const ACTIVATABLE: &str = "notification--activatable";
const NO_PROGRESS: f64 = -1.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    Plain(String),
    Markup(String),
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Notification {
    pub key: String,
    pub app_name: String,
    pub summary: String,
    pub body: Option<Body>,
    pub when: String,
    pub icon: Option<gio::Icon>,
    pub image: Option<gdk::Texture>,
    pub urgency: Urgency,
    pub actions: Vec<Action>,
    pub progress: Option<f64>,
    pub unread: bool,
    pub activatable: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Action {
    pub key: String,
    pub label: String,
}

glib::wrapper! {
    pub struct NotificationCard(ObjectSubclass<imp::NotificationCard>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for NotificationCard {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationCard {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_app_icon(&self, icon: Option<&gio::Icon>) {
        self.imp().header.set_app_icon(icon);
    }

    pub fn set_image(&self, image: Option<&gdk::Texture>) {
        self.imp().image.set_image(image);
    }

    pub fn set_actions(&self, actions: &[Action]) {
        let imp = self.imp();
        let actions = &actions[..actions.len().min(ACTIONS_MAX)];
        if *imp.shown.borrow() == actions {
            return;
        }
        imp.shown.replace(actions.to_vec());
        crate::clear_children(&imp.actions);
        for action in actions {
            imp.actions.append(&self.build_action(action));
        }
        imp.actions.set_visible(!actions.is_empty());
    }

    pub fn set_activatable(&self, activatable: bool) {
        let imp = self.imp();
        if imp.activatable.replace(activatable) == activatable {
            return;
        }
        self.set_focusable(activatable);
        crate::set_css_class(self, ACTIVATABLE, activatable);
    }

    pub fn set_notification(&self, notification: &Notification) {
        self.set_app_name(none_if_empty(&notification.app_name));
        self.set_summary(none_if_empty(&notification.summary));
        self.set_when(none_if_empty(&notification.when));
        self.set_app_icon(notification.icon.as_ref());
        self.set_image(notification.image.as_ref());
        self.set_urgency(notification.urgency);
        self.set_unread(notification.unread);
        self.set_progress(notification.progress.unwrap_or(NO_PROGRESS));
        self.set_actions(&notification.actions);
        self.set_activatable(notification.activatable);

        match notification.body.as_ref() {
            Some(Body::Markup(markup)) => self.set_body_markup(Some(markup.as_str())),
            Some(Body::Plain(text)) => self.set_body(Some(text.as_str())),
            None => self.set_body(None::<&str>),
        }
    }

    pub fn set_controls_visible(&self, visible: bool) {
        let imp = self.imp();
        imp.actions
            .set_visible(visible && !imp.shown.borrow().is_empty());
        imp.header.set_controls_visible(visible);
    }

    fn build_action(&self, action: &Action) -> gtk4::Button {
        let button = gtk4::Button::with_label(&truncate(&action.label, LABEL_MAX_CHARS));
        button.add_css_class("flat");
        button.add_css_class("notification__action");
        button.connect_clicked(glib::clone!(
            #[weak(rename_to = card)]
            self,
            #[strong(rename_to = key)]
            action.key,
            move |_| card.emit_by_name::<()>(ACTION_INVOKED, &[&key])
        ));
        button
    }

    pub fn connect_activated<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            ACTIVATED,
            false,
            glib::closure_local!(move |card: &Self| f(card)),
        )
    }

    pub fn connect_dismissed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            DISMISSED,
            false,
            glib::closure_local!(move |card: &Self| f(card)),
        )
    }

    pub fn connect_action_invoked<F: Fn(&Self, String) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            ACTION_INVOKED,
            false,
            glib::closure_local!(move |card: &Self, key: String| f(card, key)),
        )
    }
}
