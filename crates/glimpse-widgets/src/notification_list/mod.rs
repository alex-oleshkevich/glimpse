mod imp;

use adw::prelude::AnimationExt;
use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

use crate::{Action, NotificationCard, Urgency, none_if_empty, reconcile::by_key};

const ACTIVATED: &str = "activated";
const DISMISSED: &str = "dismissed";
const ACTION_INVOKED: &str = "action-invoked";

/// `progress` is a plain `f64` on the widget because a GObject property cannot be null, and
/// negative is the only value a fraction cannot otherwise take.
const NO_PROGRESS: f64 = -1.0;

/// Which of the two body setters a notification wants. The distinction is the sender's, not ours:
/// a body only becomes `Markup` after it has been through
/// `glimpse_utils::markup::sanitize_body`, and `NotificationCard` still refuses it if Pango does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    Plain(String),
    Markup(String),
}

/// `key` is the notification's own identity, not its position. Rows are matched to it across every
/// update, so a notification that moves keeps its widget — and its state, down to which action the
/// pointer was over.
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

glib::wrapper! {
    pub struct NotificationList(ObjectSubclass<imp::NotificationList>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for NotificationList {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationList {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_notifications(&self, notifications: &[Notification]) {
        let imp = self.imp();
        if imp.notifications.borrow().as_slice() == notifications {
            return;
        }
        let keep: Vec<&str> = notifications.iter().map(|n| n.key.as_str()).collect();
        imp.removals
            .borrow_mut()
            .retain(|(fading, _)| !keep.contains(&fading.key.as_str()));

        let previous = imp.notifications.replace(notifications.to_vec());
        let current_rows: Vec<String> = imp
            .rows
            .borrow()
            .iter()
            .map(|(key, _)| key.clone())
            .collect();
        let already_fading: Vec<String> = imp
            .removals
            .borrow()
            .iter()
            .map(|(fading, _)| fading.key.clone())
            .collect();
        for key in removal_keys(&current_rows, notifications, &already_fading) {
            let row = imp
                .rows
                .borrow()
                .iter()
                .find(|(held, _)| *held == key)
                .map(|(_, row)| row.clone());
            let record = previous.iter().find(|record| record.key == key).cloned();
            if let (Some(row), Some(record)) = (row, record) {
                self.start_removal(record, &row);
            }
        }

        self.reconcile();
    }

    fn start_removal(&self, notification: Notification, row: &NotificationCard) {
        let imp = self.imp();
        let animation = adw::TimedAnimation::new(
            row,
            row.opacity(),
            0.0,
            crate::duration_ms(),
            adw::PropertyAnimationTarget::new(row, "opacity"),
        );
        animation.set_easing(adw::Easing::EaseOutCubic);
        let key = notification.key.clone();
        animation.connect_done(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.finish_removal(&key)
        ));
        imp.removals
            .borrow_mut()
            .push((notification, animation.clone()));
        animation.play();
        if crate::duration_ms() == 0 {
            animation.skip();
        }
    }

    fn finish_removal(&self, key: &str) {
        let imp = self.imp();
        imp.removals
            .borrow_mut()
            .retain(|(fading, _)| fading.key != key);
        self.reconcile();
    }

    fn reconcile(&self) {
        let imp = self.imp();
        let notifications = imp.notifications.borrow().clone();
        let mut wanted = notifications.clone();
        wanted.extend(
            imp.removals
                .borrow()
                .iter()
                .map(|(fading, _)| fading.clone()),
        );

        let mut rows = imp.rows.borrow_mut();
        by_key(
            self,
            &mut rows,
            &wanted,
            |notification| notification.key.clone(),
            |notification| self.build_row(&notification.key),
            dress,
        );
        drop(rows);

        self.apply_cap();
        self.set_visible(!notifications.is_empty() || !imp.removals.borrow().is_empty());
    }

    pub fn set_cap(&self, cap: Option<usize>) {
        let imp = self.imp();
        if imp.cap.get() == cap {
            return;
        }
        imp.cap.set(cap);
        self.apply_cap();
    }

    pub fn cap(&self) -> Option<usize> {
        self.imp().cap.get()
    }

    pub fn hidden(&self) -> usize {
        let imp = self.imp();
        match imp.cap.get() {
            Some(cap) => imp.rows.borrow().len().saturating_sub(cap),
            None => 0,
        }
    }

    fn apply_cap(&self) {
        let imp = self.imp();
        let cap = imp.cap.get();
        for (index, (_, row)) in imp.rows.borrow().iter().enumerate() {
            let shown = cap.is_none_or(|cap| index < cap);
            if row.get_visible() != shown {
                row.set_visible(shown);
            }
        }
    }

    /// The key is captured when the row is built, which is safe here in a way it is not for a list
    /// that reuses rows by position: `by_key` only ever hands a row back for the same key, so the
    /// two cannot drift apart.
    fn build_row(&self, key: &str) -> NotificationCard {
        let row = NotificationCard::new();

        row.connect_activated(glib::clone!(
            #[weak(rename_to = list)]
            self,
            #[strong(rename_to = key)]
            key.to_owned(),
            move |_| list.emit_by_name::<()>(ACTIVATED, &[&key])
        ));
        row.connect_dismissed(glib::clone!(
            #[weak(rename_to = list)]
            self,
            #[strong(rename_to = key)]
            key.to_owned(),
            move |_| list.emit_by_name::<()>(DISMISSED, &[&key])
        ));
        row.connect_action_invoked(glib::clone!(
            #[weak(rename_to = list)]
            self,
            #[strong(rename_to = key)]
            key.to_owned(),
            move |_, action| list.emit_by_name::<()>(ACTION_INVOKED, &[&key, &action])
        ));

        row
    }

    pub fn connect_activated<F: Fn(&Self, String) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            ACTIVATED,
            false,
            glib::closure_local!(move |list: Self, key: String| f(&list, key)),
        )
    }

    pub fn connect_dismissed<F: Fn(&Self, String) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            DISMISSED,
            false,
            glib::closure_local!(move |list: Self, key: String| f(&list, key)),
        )
    }

    pub fn connect_action_invoked<F: Fn(&Self, String, String) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            ACTION_INVOKED,
            false,
            glib::closure_local!(move |list: Self, key: String, action: String| f(
                &list, key, action
            )),
        )
    }
}

/// Every setter here compares before it writes, so a row that has not changed costs a handful of
/// comparisons rather than a rebuild.
pub(crate) fn dress(row: &NotificationCard, notification: &Notification) {
    row.set_app_name(none_if_empty(&notification.app_name));
    row.set_summary(none_if_empty(&notification.summary));
    row.set_when(none_if_empty(&notification.when));
    row.set_app_icon(notification.icon.as_ref());
    row.set_image(notification.image.as_ref());
    row.set_urgency(notification.urgency);
    row.set_unread(notification.unread);
    row.set_progress(notification.progress.unwrap_or(NO_PROGRESS));
    row.set_actions(&notification.actions);
    row.set_activatable(notification.activatable);

    match notification.body.as_ref() {
        Some(Body::Markup(markup)) => row.set_body_markup(Some(markup.as_str())),
        Some(Body::Plain(text)) => row.set_body(Some(text.as_str())),
        None => row.set_body(None::<&str>),
    }
}

/// A displayed row starts fading out once its key drops out of the target list, unless it is
/// already fading — `set_notifications` can be called again mid-fade with the same absence.
fn removal_keys(
    current_rows: &[String],
    keep: &[Notification],
    already_fading: &[String],
) -> Vec<String> {
    current_rows
        .iter()
        .filter(|key| {
            !keep.iter().any(|notification| &notification.key == *key)
                && !already_fading.contains(key)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: &str) -> Notification {
        Notification {
            key: key.to_owned(),
            ..Notification::default()
        }
    }

    #[test]
    fn a_row_missing_from_the_target_list_and_not_already_fading_starts_removal() {
        let rows = ["a".to_owned(), "b".to_owned(), "c".to_owned()];
        let keep = [key("b")];
        let fading = ["c".to_owned()];
        assert_eq!(removal_keys(&rows, &keep, &fading), ["a".to_owned()]);
    }

    #[test]
    fn a_key_already_fading_is_not_queued_a_second_time() {
        let rows = ["a".to_owned()];
        assert_eq!(
            removal_keys(&rows, &[], &["a".to_owned()]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn clearing_every_notification_queues_every_row() {
        let rows = ["a".to_owned(), "b".to_owned()];
        assert_eq!(removal_keys(&rows, &[], &[]), rows);
    }
}
