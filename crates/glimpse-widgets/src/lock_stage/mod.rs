mod imp;

use gtk4::{gdk, gio, glib, prelude::*, subclass::prelude::*};

use crate::{
    ChipGroup, IndicatorSpec, LockClock, MessageKind, PasswordPrompt, SessionActionState,
    StatusIsland, TrackCard,
};

const INITIALS: usize = 2;

glib::wrapper! {
    pub struct LockStage(ObjectSubclass<imp::LockStage>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for LockStage {
    fn default() -> Self {
        Self::new()
    }
}

impl LockStage {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_background(&self, texture: Option<&gdk::Texture>) {
        let picture = &self.imp().background;
        let current = picture.paintable();
        let unchanged = match (current.as_ref(), texture) {
            (Some(current), Some(texture)) => current == texture.upcast_ref::<gdk::Paintable>(),
            (None, None) => true,
            _ => false,
        };
        if unchanged {
            return;
        }
        picture.set_paintable(texture);
        self.imp().blurred.set_paintable(None::<&gdk::Paintable>);
        self.imp().ensure_blurred();
    }

    pub fn set_color(&self, color: &gdk::RGBA) {
        let imp = self.imp();
        if imp.color.replace(Some(*color)) == Some(*color) {
            return;
        }
        imp.base.set_paintable(Some(&crate::raster::solid(*color)));
    }

    pub fn set_fit(&self, fit: gtk4::ContentFit) {
        let imp = self.imp();
        if imp.background.content_fit() == fit {
            return;
        }
        imp.background.set_content_fit(fit);
        imp.blurred.set_content_fit(fit);
    }

    pub fn set_dim(&self, dim: f64) {
        if dim.is_nan() {
            return;
        }
        let imp = self.imp();
        let dim = dim.clamp(0.0, 1.0);
        if imp.dim.replace(dim) == dim {
            return;
        }
        imp.scrim.set_opacity(dim);
    }

    pub fn set_interactive(&self, interactive: bool) {
        self.imp().interactive.set(interactive);
        self.imp().prompt.set_interactive(interactive);
        if !interactive {
            self.set_awake(false);
        }
    }

    pub fn set_awake(&self, awake: bool) {
        self.imp().set_awake(awake);
    }

    pub fn is_awake(&self) -> bool {
        self.imp().awake.get()
    }

    pub fn set_user(&self, user: Option<&str>) {
        let imp = self.imp();
        if imp.user.borrow().as_deref() == user {
            return;
        }
        imp.user.replace(user.map(ToOwned::to_owned));
        self.prompt().set_user(user);
        let initials = initials(user.unwrap_or_default());
        let label = &self.imp().initials;
        if label.text().as_str() != initials {
            label.set_text(&initials);
        }
    }

    pub fn set_message(&self, text: Option<&str>, kind: MessageKind) {
        let imp = self.imp();
        let incoming = (text.map(ToOwned::to_owned), kind);
        if *imp.message.borrow() == incoming {
            return;
        }
        imp.message.replace(incoming);
        let changed = imp.prompt.set_message(text, kind);
        if !changed || text.is_none_or(|text| text.trim().is_empty()) {
            return;
        }
        let shown = imp.awake.get();
        self.set_awake(true);
        if shown {
            imp.touch();
        }
    }

    pub fn set_busy(&self, busy: bool) {
        let imp = self.imp();
        let was = imp.prompt.is_busy();
        imp.prompt.set_busy(busy);
        if was && !busy && imp.awake.get() {
            imp.touch();
        }
    }

    pub fn set_avatar(&self, texture: Option<&gdk::Texture>) {
        let photo = &self.imp().photo;
        let current = photo.paintable();
        if current.as_ref() == texture.map(|texture| texture.upcast_ref::<gdk::Paintable>()) {
            return;
        }
        photo.set_paintable(texture);
        photo.set_visible(texture.is_some());
    }

    pub fn set_battery(&self, spec: Option<&IndicatorSpec>) {
        let imp = self.imp();
        imp.status.set_battery(spec);
        match spec {
            Some(spec) => {
                imp.cover_battery.apply(spec);
                if !imp.cover_battery.get_visible() {
                    imp.cover_battery.set_visible(true);
                }
            }
            None if imp.cover_battery.get_visible() => imp.cover_battery.set_visible(false),
            None => {}
        }
    }

    pub fn set_notifications(&self, groups: &[ChipGroup]) {
        let imp = self.imp();
        if crate::status_island::same_groups(&imp.groups.borrow(), groups) {
            return;
        }
        imp.groups.replace(groups.to_vec());
        imp.status.set_notifications(groups);
        let count = groups
            .iter()
            .fold(0u32, |total, group| total.saturating_add(group.count));
        if count == 0 {
            if imp.cover_notifications.get_visible() {
                imp.cover_notifications.set_visible(false);
            }
            return;
        }
        imp.cover_notifications.apply(&IndicatorSpec {
            icon: Some(gio::ThemedIcon::new("preferences-system-notifications-symbolic").upcast()),
            label: Some(count.to_string()),
            tooltip: Some(crate::status_island::count_phrase(count)),
            ..Default::default()
        });
        if !imp.cover_notifications.get_visible() {
            imp.cover_notifications.set_visible(true);
        }
    }

    pub fn set_session_action(&self, action: &str, state: &SessionActionState) {
        self.set_session_actions([(action, state)]);
    }

    pub fn set_session_actions<'a>(
        &self,
        actions: impl IntoIterator<Item = (&'a str, &'a SessionActionState)>,
    ) {
        let imp = self.imp();
        for (action, state) in actions {
            imp.sheet.set_action(action, state);
        }
        let available = imp.sheet.has_actions();
        imp.status.set_session_available(available);
        if !available {
            imp.sheet.close();
        }
    }

    pub fn set_session_error(&self, text: Option<&str>) {
        let sheet = &self.imp().sheet;
        if !sheet.is_open() {
            return;
        }
        sheet.set_error(text);
    }

    pub fn session_open(&self) -> bool {
        self.imp().sheet.is_open()
    }

    pub fn close_session(&self) {
        self.imp().sheet.close();
    }

    pub fn status(&self) -> &StatusIsland {
        &self.imp().status
    }

    pub fn clock(&self) -> &LockClock {
        &self.imp().clock
    }

    pub fn prompt(&self) -> &PasswordPrompt {
        &self.imp().prompt
    }

    pub fn track(&self) -> &TrackCard {
        &self.imp().track
    }

    pub fn connect_session_action<F: Fn(&Self, &str) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "session-action",
            false,
            glib::closure_local!(move |stage: Self, action: String| f(&stage, &action)),
        )
    }
}

fn initials(name: &str) -> String {
    glimpse_utils::clean(name, crate::password_prompt::USER_MAX_CHARS)
        .split_whitespace()
        .filter_map(|word| word.chars().next())
        .flat_map(char::to_uppercase)
        .take(INITIALS)
        .collect()
}

#[cfg(test)]
mod tests;
