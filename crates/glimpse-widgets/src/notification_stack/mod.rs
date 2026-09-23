mod imp;

use adw::prelude::*;
use gettextrs::{gettext, ngettext};
use gtk4::{gdk, glib, subclass::prelude::*};

use crate::notification_list::dress;
use crate::{Notification, NotificationCard};

#[cfg(test)]
pub(crate) use imp::MAX_DEPTH;

const ACTIVATED: &str = "activated";
const DISMISSED: &str = "dismissed";
const ACTION_INVOKED: &str = "action-invoked";
const CLEAR_REQUESTED: &str = "clear-requested";

pub(crate) const STACK_MIN_ITEMS: usize = 4;

const STRIP: &str = "notification-stack__strip";
const STRIP_FAR: &str = "notification-stack__strip--far";
const EXPAND_MILLIS: u32 = 200;
const COLLAPSE_MILLIS: u32 = 150;

glib::wrapper! {
    pub struct NotificationStack(ObjectSubclass<imp::NotificationStack>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for NotificationStack {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationStack {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_items(&self, notifications: &[Notification]) {
        let imp = self.imp();
        if imp.notifications.borrow().as_slice() == notifications {
            return;
        }
        let was_stackable = imp.notifications.borrow().len() >= STACK_MIN_ITEMS;
        imp.notifications.replace(notifications.to_vec());
        let stackable = notifications.len() >= STACK_MIN_ITEMS;
        if !stackable {
            imp.collapsed.set(false);
        } else if !was_stackable {
            imp.collapsed.set(true);
        }
        self.snap_transition();
        self.rebuild();
    }

    pub fn set_collapsed(&self, collapsed: bool) {
        let imp = self.imp();
        let collapsed = collapsed && imp.notifications.borrow().len() >= STACK_MIN_ITEMS;
        if imp.collapsed.get() == collapsed {
            return;
        }
        imp.collapsed.set(collapsed);
        if !imp.animated.get() {
            self.snap_transition();
            self.rebuild();
            return;
        }

        self.sync_rows();
        self.sync_strips();
        self.sync_chip();
        self.arrange();
        self.animate_transition();
    }

    pub fn is_collapsed(&self) -> bool {
        self.imp().collapsed.get()
    }

    pub fn set_animated(&self, animated: bool) {
        let imp = self.imp();
        if imp.animated.replace(animated) == animated {
            return;
        }
        if !animated {
            self.snap_transition();
            self.rebuild();
        }
    }

    pub fn header_control(&self) -> gtk4::Button {
        let imp = self.imp();
        let chip = imp.chip.get().expect("notification stack chip").clone();
        if !imp.chip_external.replace(true) {
            if chip.parent().is_some() {
                chip.unparent();
            }
            self.queue_resize();
        }
        chip
    }

    fn rebuild(&self) {
        let imp = self.imp();
        let notifications = imp.notifications.borrow().clone();

        {
            let mut rows = imp.rows.borrow_mut();
            let mut next: Vec<(String, NotificationCard)> = Vec::with_capacity(notifications.len());
            for notification in &notifications {
                let row = match rows.iter().position(|(key, _)| *key == notification.key) {
                    Some(at) => rows.remove(at).1,
                    None => self.build_row(&notification.key),
                };
                dress(&row, notification);
                next.push((notification.key.clone(), row));
            }
            for (_, row) in rows.drain(..) {
                row.unparent();
            }
            *rows = next;
        }

        self.sync_rows();
        self.sync_strips();
        self.sync_chip();
        self.arrange();
        self.set_visible(!notifications.is_empty());
        self.queue_resize();
    }

    fn sync_rows(&self) {
        let imp = self.imp();
        let collapsed = imp.collapsed.get();
        let transitioning = self.is_transitioning();
        let progress = imp.progress.get();
        let notifications = imp.notifications.borrow();
        let rows = imp.rows.borrow();
        let preview = collapsed && rows.len() >= STACK_MIN_ITEMS;
        for (index, (_, row)) in rows.iter().enumerate() {
            let shown = !collapsed || transitioning || index == 0;
            if row.get_visible() != shown {
                row.set_visible(shown);
            }
            row.set_opacity(if index == 0 || !transitioning {
                1.0
            } else {
                progress
            });
            row.set_can_target(index == 0 || !transitioning);
            row.set_controls_visible(!preview);
            row.set_activatable(
                preview && index == 0
                    || notifications
                        .get(index)
                        .is_some_and(|notification| notification.activatable),
            );
        }
    }

    fn sync_strips(&self) {
        let imp = self.imp();
        let wanted = imp.visual_depth();
        let mut strips = imp.strips.borrow_mut();

        while strips.len() < wanted {
            let strip = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            strip.add_css_class(STRIP);
            strips.push(strip);
        }
        for strip in strips.split_off(wanted) {
            strip.unparent();
        }

        let count = strips.len();
        for (index, strip) in strips.iter().enumerate() {
            crate::set_css_class(strip, STRIP_FAR, count > 1 && index == 0);
            strip.set_opacity(1.0 - imp.progress.get());
        }
    }

    fn sync_chip(&self) {
        let imp = self.imp();
        let count = imp.rows.borrow().len();
        let shown = count >= STACK_MIN_ITEMS;

        let Some(chip) = imp.chip.get() else {
            return;
        };
        if chip.get_visible() != shown {
            chip.set_visible(shown);
        }
        if !shown {
            return;
        }

        let collapsed = imp.collapsed.get();
        let text = if collapsed {
            summary(count)
        } else {
            gettext("Collapse")
        };
        if let Some(label) = imp.chip_label.get()
            && label.text() != text
        {
            label.set_text(&text);
        }

        let arrow = if collapsed {
            "pan-down-symbolic"
        } else {
            "pan-up-symbolic"
        };
        if let Some(image) = imp.chip_arrow.get()
            && image.icon_name().as_deref() != Some(arrow)
        {
            image.set_icon_name(Some(arrow));
        }
    }

    fn arrange(&self) {
        let imp = self.imp();
        let mut order: Vec<gtk4::Widget> = Vec::new();
        let transitioning = self.is_transitioning();
        let rows = imp.rows.borrow();
        if transitioning {
            order.extend(rows.iter().skip(1).map(|(_, row)| row.clone().upcast()));
        }
        order.extend(
            imp.strips
                .borrow()
                .iter()
                .map(|strip| strip.clone().upcast()),
        );
        if let Some((_, front)) = rows.first() {
            order.push(front.clone().upcast());
        }
        if !transitioning {
            order.extend(rows.iter().skip(1).map(|(_, row)| row.clone().upcast()));
        }
        drop(rows);
        if !imp.chip_external.get()
            && let Some(chip) = imp.chip.get()
        {
            order.push(chip.clone().upcast());
        }

        let mut previous: Option<gtk4::Widget> = None;
        for widget in &order {
            if widget.parent().is_none() || widget.prev_sibling() != previous {
                if widget.parent().is_some() {
                    widget.unparent();
                }
                widget.insert_after(self, previous.as_ref());
            }
            previous = Some(widget.clone());
        }
    }

    fn build_row(&self, key: &str) -> NotificationCard {
        let row = NotificationCard::new();

        row.connect_activated(glib::clone!(
            #[weak(rename_to = stack)]
            self,
            #[strong(rename_to = key)]
            key.to_owned(),
            move |_| {
                if stack.expand_preview() {
                    return;
                }
                stack.emit_by_name::<()>(ACTIVATED, &[&key]);
            }
        ));
        row.connect_dismissed(glib::clone!(
            #[weak(rename_to = stack)]
            self,
            #[strong(rename_to = key)]
            key.to_owned(),
            move |_| stack.emit_by_name::<()>(DISMISSED, &[&key])
        ));
        row.connect_action_invoked(glib::clone!(
            #[weak(rename_to = stack)]
            self,
            #[strong(rename_to = key)]
            key.to_owned(),
            move |_, action| {
                if stack.expand_preview() {
                    return;
                }
                stack.emit_by_name::<()>(ACTION_INVOKED, &[&key, &action]);
            }
        ));

        let secondary = gtk4::GestureClick::new();
        secondary.set_button(gdk::BUTTON_SECONDARY);
        secondary.connect_released(glib::clone!(
            #[weak(rename_to = stack)]
            self,
            #[strong(rename_to = key)]
            key.to_owned(),
            move |gesture, _, _, _| {
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                if stack.is_collapsed() {
                    stack.emit_by_name::<()>(CLEAR_REQUESTED, &[]);
                } else {
                    stack.emit_by_name::<()>(DISMISSED, &[&key]);
                }
            }
        ));
        row.add_controller(secondary);

        row
    }

    fn expand_preview(&self) -> bool {
        if !self.is_collapsed() || self.imp().rows.borrow().len() < STACK_MIN_ITEMS {
            return false;
        }
        self.set_collapsed(false);
        true
    }

    fn animate_transition(&self) {
        let imp = self.imp();
        let from = imp.progress.get();
        let to: f64 = if imp.collapsed.get() { 0.0 } else { 1.0 };
        let duration = crate::animation_ms(if imp.collapsed.get() {
            COLLAPSE_MILLIS
        } else {
            EXPAND_MILLIS
        });
        let animation = self.animation();
        animation.reset();
        animation.set_value_from(from);
        animation.set_value_to(to);
        animation.set_duration(((duration as f64 * (to - from).abs()).round() as u32).max(1));
        self.set_transition_progress(from);
        animation.play();
        if duration == 0 {
            animation.skip();
        }
    }

    fn animation(&self) -> adw::TimedAnimation {
        if let Some(animation) = self.imp().animation.get() {
            return animation.clone();
        }

        let target = adw::CallbackAnimationTarget::new(glib::clone!(
            #[weak(rename_to = stack)]
            self,
            move |value| stack.set_transition_progress(value)
        ));
        let animation = adw::TimedAnimation::new(self, 0.0, 0.0, EXPAND_MILLIS, target);
        animation.set_easing(adw::Easing::EaseOutCubic);
        animation.set_follow_enable_animations_setting(true);
        animation.connect_done(glib::clone!(
            #[weak(rename_to = stack)]
            self,
            move |_| stack.finish_transition()
        ));
        let _ = self.imp().animation.set(animation.clone());
        animation
    }

    fn snap_transition(&self) {
        let imp = self.imp();
        if let Some(animation) = imp.animation.get() {
            animation.reset();
        }
        imp.progress
            .set(if imp.collapsed.get() { 0.0 } else { 1.0 });
    }

    pub(crate) fn set_transition_progress(&self, progress: f64) {
        let progress = progress.clamp(0.0, 1.0);
        let imp = self.imp();
        imp.progress.set(progress);
        let rows: Vec<_> = imp
            .rows
            .borrow()
            .iter()
            .skip(1)
            .map(|(_, row)| row.clone())
            .collect();
        let strips = imp.strips.borrow().clone();
        for row in rows {
            row.set_opacity(progress);
        }
        for strip in strips {
            strip.set_opacity(1.0 - progress);
        }
        self.queue_resize();
    }

    pub(crate) fn finish_transition(&self) {
        self.imp()
            .progress
            .set(if self.is_collapsed() { 0.0 } else { 1.0 });
        self.sync_rows();
        self.sync_strips();
        self.arrange();
        self.queue_resize();
    }

    fn is_transitioning(&self) -> bool {
        let imp = self.imp();
        let target = if imp.collapsed.get() { 0.0 } else { 1.0 };
        (imp.progress.get() - target).abs() > f64::EPSILON
    }

    pub fn connect_activated<F: Fn(&Self, String) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            ACTIVATED,
            false,
            glib::closure_local!(move |stack: Self, key: String| f(&stack, key)),
        )
    }

    pub fn connect_dismissed<F: Fn(&Self, String) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            DISMISSED,
            false,
            glib::closure_local!(move |stack: Self, key: String| f(&stack, key)),
        )
    }

    pub fn connect_action_invoked<F: Fn(&Self, String, String) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            ACTION_INVOKED,
            false,
            glib::closure_local!(move |stack: Self, key: String, action: String| f(
                &stack, key, action
            )),
        )
    }

    pub fn connect_clear_requested<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            CLEAR_REQUESTED,
            false,
            glib::closure_local!(move |stack: Self| f(&stack)),
        )
    }
}

fn summary(count: usize) -> String {
    ngettext(
        "{count} notification",
        "{count} notifications",
        count as u32,
    )
    .replace("{count}", &count.to_string())
}

#[cfg(test)]
mod tests {
    use super::{imp::interpolate, summary};

    #[test]
    fn the_chip_counts_what_is_behind_the_front_card_as_well_as_the_front_card() {
        assert_eq!(summary(2), "2 notifications");
        assert_eq!(summary(1), "1 notification");
    }

    #[test]
    fn stack_geometry_interpolates_between_collapsed_and_expanded_positions() {
        assert_eq!(interpolate(80, 240, 0.0), 80);
        assert_eq!(interpolate(80, 240, 0.5), 160);
        assert_eq!(interpolate(80, 240, 1.0), 240);
    }
}
