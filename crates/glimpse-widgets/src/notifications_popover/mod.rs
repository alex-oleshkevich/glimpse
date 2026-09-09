mod imp;

use gettextrs::{gettext, ngettext};
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::reconcile::by_key;
use crate::{Notification, NotificationList, Row, Section};

const ACTIVATED: &str = "activated";
const DISMISSED: &str = "dismissed";
const ACTION_INVOKED: &str = "action-invoked";
const DND_TOGGLED: &str = "dnd-toggled";
const CLEAR_ALL: &str = "clear-all";
const FOOTER_ACTIVATED: &str = "footer-activated";
const CLEAR_GROUP: &str = "clear-group";

/// GNOME collapses a group to three. Past that a popover holding several noisy applications is a
/// scroll rather than a summary — and every board in this crate is hand-authored at four to ten
/// items, which is why it went unseen.
const GROUP_CAP: usize = 3;

/// One application's notifications. `key` is what grouping is done on and is the sender's identity
/// — its desktop entry or bus name — rather than `app_name`, which the sender chooses and can
/// therefore borrow from somebody else.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Group {
    pub key: String,
    pub app_name: String,
    pub notifications: Vec<Notification>,
}

glib::wrapper! {
    pub struct NotificationsPopover(ObjectSubclass<imp::NotificationsPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for NotificationsPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationsPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_groups(&self, groups: &[Group]) {
        let imp = self.imp();
        if imp.held.borrow().as_slice() == groups {
            return;
        }
        imp.held.replace(groups.to_vec());

        let mut sections = imp.sections.borrow_mut();
        by_key(
            &*imp.groups,
            &mut sections,
            groups,
            |group| group.key.clone(),
            |group| self.section(&group.key),
            |section, group| {
                section.set_title(Some(group.app_name.as_str()));
                section.set_count(count(group.notifications.len()).as_deref());
                let Some(list) = descendant::<NotificationList>(section) else {
                    return;
                };
                list.set_notifications(&group.notifications);
                if group.notifications.len() <= GROUP_CAP {
                    list.set_cap(Some(GROUP_CAP));
                }
                if let Some(more) = descendant::<Row>(section) {
                    dress_more(&more, &list);
                }
            },
        );
        drop(sections);

        let anything = groups.iter().any(|group| !group.notifications.is_empty());
        imp.groups.set_visible(anything);
        imp.empty.set_visible(!anything);
        imp.clear.set_visible(anything);
    }

    /// A section carries its own list rather than the popover holding a second collection beside
    /// `sections`: two structures keyed the same way are two chances to disagree.
    fn section(&self, key: &str) -> Section {
        let section = Section::new();
        let list = NotificationList::new();

        list.connect_activated(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_, key| popover.emit_by_name::<()>(ACTIVATED, &[&key])
        ));
        list.connect_dismissed(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_, key| popover.emit_by_name::<()>(DISMISSED, &[&key])
        ));
        list.connect_action_invoked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_, key, action| popover.emit_by_name::<()>(ACTION_INVOKED, &[&key, &action])
        ));

        let more = Row::new();
        more.set_visible(false);
        more.add_css_class("section__more");
        more.connect_clicked(glib::clone!(
            #[weak]
            list,
            #[weak]
            more,
            move |_| {
                let expanded = list.cap().is_none();
                list.set_cap(expanded.then_some(GROUP_CAP));
                dress_more(&more, &list);
            }
        ));

        let body = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        body.append(&list);
        body.append(&more);

        let clear = gtk4::Button::builder()
            .icon_name("user-trash-symbolic")
            .tooltip_text(gettext("Clear these notifications"))
            .has_frame(false)
            .valign(gtk4::Align::Center)
            .build();
        clear.add_css_class("section__clear");
        clear.connect_clicked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            #[strong(rename_to = key)]
            key.to_owned(),
            move |_| popover.emit_by_name::<()>(CLEAR_GROUP, &[&key])
        ));

        list.set_cap(Some(GROUP_CAP));
        section.set_content(Some(&body));
        section.set_trail(Some(&clear));
        section
    }

    /// Do not disturb, as the popover shows it. Setting it never reports back — see `echoing`.
    pub fn set_dnd(&self, silenced: bool) {
        let imp = self.imp();
        if imp.quiet.is_active() == silenced {
            return;
        }
        imp.echoing.set(true);
        imp.quiet.set_active(silenced);
        imp.echoing.set(false);
    }

    pub fn dnd(&self) -> bool {
        self.imp().quiet.is_active()
    }

    /// The wording of the failure is the widget's, because it is the same failure every time; the
    /// detail is the caller's, because only it knows which name was taken or which bus was down.
    pub fn set_trouble(&self, detail: Option<&str>) {
        let imp = self.imp();
        imp.trouble.set_subtitle(detail);
        imp.trouble.set_visible(detail.is_some());
    }

    pub fn set_clear_label(&self, label: Option<&str>) {
        crate::set_footer_row(&self.imp().clear, label);
    }

    pub fn set_footer(&self, label: Option<&str>) {
        crate::set_footer_row(&self.imp().footer, label);
    }

    pub fn connect_activated<F: Fn(&Self, String) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.forward_key(ACTIVATED, f)
    }

    pub fn connect_dismissed<F: Fn(&Self, String) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.forward_key(DISMISSED, f)
    }

    fn forward_key<F: Fn(&Self, String) + 'static>(
        &self,
        signal: &str,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            signal,
            false,
            glib::closure_local!(move |popover: Self, key: String| f(&popover, key)),
        )
    }

    pub fn connect_action_invoked<F: Fn(&Self, String, String) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            ACTION_INVOKED,
            false,
            glib::closure_local!(move |popover: Self, key: String, action: String| f(
                &popover, key, action
            )),
        )
    }

    pub fn connect_dnd_toggled<F: Fn(&Self, bool) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            DND_TOGGLED,
            false,
            glib::closure_local!(move |popover: Self, silenced: bool| f(&popover, silenced)),
        )
    }

    pub fn connect_clear_group<F: Fn(&Self, String) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.forward_key(CLEAR_GROUP, f)
    }

    pub fn connect_clear_all<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            CLEAR_ALL,
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }

    pub fn connect_footer_activated<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            FOOTER_ACTIVATED,
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }
}

/// One is not worth a number beside a name that already says it.
fn count(notifications: usize) -> Option<String> {
    (notifications > 1).then(|| {
        ngettext("{count}", "{count}", notifications as u32)
            .replace("{count}", &notifications.to_string())
    })
}

/// The control that opens a group is the one that closes it, and it takes itself away when there
/// is nothing left behind it.
fn dress_more(more: &Row, list: &NotificationList) {
    let label = match list.cap() {
        None => Some(gettext("Show less")),
        Some(_) => {
            let hidden = list.hidden();
            (hidden > 0).then(|| {
                ngettext(
                    "{count} more notification",
                    "{count} more notifications",
                    hidden as u32,
                )
                .replace("{count}", &hidden.to_string())
            })
        }
    };
    crate::set_footer_row(more, label.as_deref());
}

fn descendant<T: IsA<gtk4::Widget>>(root: &impl IsA<gtk4::Widget>) -> Option<T> {
    let widget = root.upcast_ref::<gtk4::Widget>();
    if let Some(found) = widget.downcast_ref::<T>() {
        return Some(found.clone());
    }
    let mut child = widget.first_child();
    while let Some(node) = child {
        if let Some(found) = descendant::<T>(&node) {
            return Some(found);
        }
        child = node.next_sibling();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::count;

    #[test]
    fn a_single_notification_is_not_counted_beside_the_name_that_already_says_it() {
        assert_eq!(count(0), None);
        assert_eq!(count(1), None);
        assert_eq!(count(2).as_deref(), Some("2"));
    }
}
