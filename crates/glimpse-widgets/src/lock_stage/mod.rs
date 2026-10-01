mod imp;

use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

use crate::{
    LockClock, NotificationChips, PasswordPrompt, SessionActionState, StatusIsland, TrackCard,
};

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
    }

    pub fn set_color(&self, color: &gdk::RGBA) {
        let imp = self.imp();
        if imp.color.replace(Some(*color)) == Some(*color) {
            return;
        }
        imp.base.set_paintable(Some(&crate::raster::solid(*color)));
    }

    pub fn set_fit(&self, fit: gtk4::ContentFit) {
        let picture = &self.imp().background;
        if picture.content_fit() == fit {
            return;
        }
        picture.set_content_fit(fit);
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
        self.imp().prompt.set_interactive(interactive);
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

    pub fn chips(&self) -> &NotificationChips {
        &self.imp().chips
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

#[cfg(test)]
mod tests;
