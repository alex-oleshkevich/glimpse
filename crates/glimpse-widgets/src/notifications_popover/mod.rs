mod imp;

use gettextrs::gettext;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::reconcile::by_key;
use crate::{Notification, NotificationStack, Section};

const ACTIVATED: &str = "activated";
const DISMISSED: &str = "dismissed";
const ACTION_INVOKED: &str = "action-invoked";
const DND_TOGGLED: &str = "dnd-toggled";
const CLEAR_ALL: &str = "clear-all";
const FOOTER_ACTIVATED: &str = "footer-activated";
const CLEAR_GROUP: &str = "clear-group";

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
                let Some(stack) = descendant::<NotificationStack>(section) else {
                    return;
                };
                stack.set_items(&group.notifications);
                if let Some(clear) =
                    descendant_with_class::<gtk4::Button>(section, "section__clear")
                {
                    clear.set_visible(
                        group.notifications.len() >= crate::notification_stack::STACK_MIN_ITEMS,
                    );
                }
            },
        );
        drop(sections);

        let anything = groups.iter().any(|group| !group.notifications.is_empty());
        imp.groups.set_visible(anything);
        imp.empty.set_visible(!anything);
        imp.clear.set_visible(anything);
    }

    fn section(&self, key: &str) -> Section {
        let section = Section::new();
        section.add_css_class("notifications-popover__group");
        let stack = NotificationStack::new();

        stack.connect_activated(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_, key| popover.emit_by_name::<()>(ACTIVATED, &[&key])
        ));
        stack.connect_dismissed(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_, key| popover.emit_by_name::<()>(DISMISSED, &[&key])
        ));
        stack.connect_action_invoked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_, key, action| popover.emit_by_name::<()>(ACTION_INVOKED, &[&key, &action])
        ));
        stack.connect_clear_requested(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            #[strong(rename_to = key)]
            key.to_owned(),
            move |_| popover.emit_by_name::<()>(CLEAR_GROUP, &[&key])
        ));

        let clear = gtk4::Button::builder()
            .icon_name("window-close-symbolic")
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

        let trail = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        trail.add_css_class("notifications-popover__group-actions");
        trail.append(&stack.header_control());
        trail.append(&clear);

        section.set_content(Some(&stack));
        section.set_trail(Some(&trail));
        section
    }

    pub fn set_dnd(&self, silenced: bool) {
        let imp = self.imp();
        let enabled = !silenced;
        if imp.notifications.is_active() == enabled {
            return;
        }
        imp.echoing.set(true);
        imp.notifications.set_active(enabled);
        imp.echoing.set(false);
    }

    pub fn dnd(&self) -> bool {
        !self.imp().notifications.is_active()
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

fn descendant_with_class<T: IsA<gtk4::Widget>>(
    root: &impl IsA<gtk4::Widget>,
    class: &str,
) -> Option<T> {
    let widget = root.upcast_ref::<gtk4::Widget>();
    if widget.has_css_class(class) {
        return widget.clone().downcast().ok();
    }
    let mut child = widget.first_child();
    while let Some(node) = child {
        if let Some(found) = descendant_with_class::<T>(&node, class) {
            return Some(found);
        }
        child = node.next_sibling();
    }
    None
}
