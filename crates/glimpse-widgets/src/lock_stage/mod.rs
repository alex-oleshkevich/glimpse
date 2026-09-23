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
mod tests {
    use super::*;
    use glib::translate::IntoGlib;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    fn shown(visible: bool) -> SessionActionState {
        SessionActionState {
            visible,
            enabled: true,
            subtitle: None,
        }
    }

    fn press(stage: &LockStage, key: gdk::Key) -> bool {
        let keys = stage
            .observe_controllers()
            .into_iter()
            .filter_map(Result::ok)
            .find_map(|controller| controller.downcast::<gtk4::EventControllerKey>().ok())
            .expect("the stage carries a key controller");
        assert_eq!(
            keys.propagation_phase(),
            gtk4::PropagationPhase::Capture,
            "the key controller sees Escape before any child"
        );
        keys.emit_by_name::<bool>(
            "key-pressed",
            &[&key.into_glib(), &0u32, &gdk::ModifierType::empty()],
        )
    }

    fn prompt_focused(window: &gtk4::Window, stage: &LockStage) -> bool {
        RootExt::focus(window).is_some_and(|focus| focus.is_ancestor(stage.prompt()))
    }

    #[test]
    #[ignore = "needs a display"]
    fn lock_stage_states() {
        if gtk4::init().is_err() {
            return;
        }
        crate::register_resources().expect("resources");

        let stage = LockStage::new();
        let window = gtk4::Window::new();
        window.set_child(Some(&stage));
        let imp = stage.imp();

        assert!(
            stage.has_css_class("lock-stage"),
            "the class every lock rule is scoped under"
        );
        let (min_height, _, _, _) = stage.measure(gtk4::Orientation::Vertical, -1);
        assert!(min_height > 0, "the stage asks for its layout's minimum");

        let huge = gdk::MemoryTexture::new(
            3840,
            2160,
            gdk::MemoryFormat::R8g8b8a8,
            &glib::Bytes::from_owned(vec![0u8; 3840 * 2160 * 4]),
            3840 * 4,
        );
        let empty = LockStage::new();
        let (_, empty_width, _, _) = empty.measure(gtk4::Orientation::Horizontal, -1);
        let photo = LockStage::new();
        photo.set_background(Some(huge.upcast_ref::<gdk::Texture>()));
        let (_, natural_width, _, _) = photo.measure(gtk4::Orientation::Horizontal, -1);
        let (_, natural_height, _, _) = photo.measure(gtk4::Orientation::Vertical, -1);
        assert_eq!(
            natural_width, empty_width,
            "a 4K texture does not widen the stage"
        );
        assert!(
            natural_width < 3840 && natural_height < 2160,
            "the stage's natural size is its layout's, not the texture's"
        );

        assert!(
            imp.base.paintable().is_some(),
            "a fresh stage paints a colour under an empty image"
        );
        let rebuilt = Rc::new(Cell::new(0));
        imp.base.connect_paintable_notify({
            let rebuilt = Rc::clone(&rebuilt);
            move |_| rebuilt.set(rebuilt.get() + 1)
        });
        stage.set_color(&gdk::RGBA::BLACK);
        assert_eq!(rebuilt.get(), 0, "the default colour again builds nothing");
        stage.set_color(&gdk::RGBA::RED);
        assert_eq!(rebuilt.get(), 1, "a new colour builds one texture");
        stage.set_color(&gdk::RGBA::RED);
        assert_eq!(rebuilt.get(), 1, "the same colour again builds nothing");

        let power = || {
            let mut found = None;
            let mut child = imp.status.first_child();
            while let Some(widget) = child {
                if widget.has_css_class("status-island__power") {
                    found = Some(widget.clone());
                }
                child = widget.next_sibling();
            }
            found.expect("the island has a power button")
        };
        assert!(
            !power().get_visible(),
            "a sheet with no actions hides the power button from the start"
        );
        stage.set_session_action(crate::SUSPEND, &shown(true));
        assert!(
            power().get_visible(),
            "one action makes the island offer the sheet"
        );
        stage.set_session_action(crate::SUSPEND, &shown(false));
        assert!(
            !power().get_visible(),
            "the last hidden action hides it again"
        );
        stage.set_session_action(crate::SUSPEND, &shown(true));
        let error = &imp.sheet.imp().error;
        stage.set_session_error(Some("refused"));
        assert!(
            !error.get_visible(),
            "an error for a closed sheet is dropped"
        );
        imp.status.emit_by_name::<()>("session-toggled", &[]);
        stage.set_session_error(Some("refused"));
        assert!(error.get_visible(), "an open sheet shows the error");
        stage.set_session_action(crate::SUSPEND, &shown(false));
        assert!(
            !imp.sheet.is_open(),
            "losing the last action closes an open sheet"
        );
        stage.set_session_action(crate::SUSPEND, &shown(true));
        imp.status.emit_by_name::<()>("session-toggled", &[]);
        assert!(stage.session_open());
        stage.set_session_actions([
            (crate::SUSPEND, &shown(false)),
            (crate::REBOOT, &shown(true)),
        ]);
        assert!(
            imp.sheet.is_open(),
            "the sheet closes only when no action is left after the whole batch"
        );
        imp.status.emit_by_name::<()>("session-toggled", &[]);
        assert!(!stage.session_open());
        stage.set_session_action(crate::SUSPEND, &shown(true));
        stage.set_session_action(crate::REBOOT, &shown(true));

        assert!(stage.prompt().grab_focus());
        assert!(
            !press(&stage, gdk::Key::Escape),
            "Escape on a closed sheet propagates"
        );

        imp.status.emit_by_name::<()>("session-toggled", &[]);
        assert!(
            imp.sheet.is_open(),
            "the island's power button opens the sheet"
        );
        assert!(
            !prompt_focused(&window, &stage),
            "an open sheet takes the focus"
        );
        assert!(
            !press(&stage, gdk::Key::a),
            "a key other than Escape propagates"
        );
        assert!(imp.sheet.is_open());

        assert!(
            press(&stage, gdk::Key::Escape),
            "Escape on an open sheet stops there"
        );
        assert!(!imp.sheet.is_open(), "Escape closes the sheet");
        assert!(
            prompt_focused(&window, &stage),
            "closing the sheet hands the focus back to the prompt"
        );

        imp.status.emit_by_name::<()>("session-toggled", &[]);
        assert!(imp.sheet.is_open());
        imp.status.emit_by_name::<()>("session-toggled", &[]);
        assert!(
            !imp.sheet.is_open(),
            "the power button toggles the sheet shut"
        );

        let actions = Rc::new(RefCell::new(Vec::new()));
        stage.connect_session_action({
            let actions = Rc::clone(&actions);
            move |_, action| actions.borrow_mut().push(action.to_owned())
        });
        imp.sheet
            .emit_by_name::<()>("action-requested", &[&crate::SUSPEND]);
        assert_eq!(*actions.borrow(), [crate::SUSPEND]);

        imp.status.emit_by_name::<()>("session-toggled", &[]);
        stage.close_session();
        assert!(!imp.sheet.is_open(), "close_session closes an open sheet");

        stage.set_dim(0.4);
        assert!((imp.scrim.opacity() - 0.4).abs() < 0.01);
        stage.set_dim(f64::NAN);
        assert!(
            (imp.scrim.opacity() - 0.4).abs() < 0.01,
            "a NaN dim is ignored rather than written"
        );
        stage.set_dim(3.0);
        assert!((imp.scrim.opacity() - 1.0).abs() < 0.01, "dim clamps to 1");
        stage.set_dim(-1.0);
        assert!(imp.scrim.opacity().abs() < 0.01, "dim clamps to 0");

        let texture = gdk::MemoryTexture::new(
            2,
            2,
            gdk::MemoryFormat::R8g8b8a8,
            &glib::Bytes::from_static(&[0; 16]),
            8,
        );
        stage.set_background(Some(texture.upcast_ref::<gdk::Texture>()));
        assert!(imp.background.paintable().is_some());
        stage.set_background(None);
        assert!(imp.background.paintable().is_none());
        stage.set_fit(gtk4::ContentFit::Contain);
        assert_eq!(imp.background.content_fit(), gtk4::ContentFit::Contain);

        stage.set_interactive(false);
        assert!(
            !stage.prompt().grab_focus(),
            "a non-interactive stage's prompt refuses focus"
        );
        imp.status.emit_by_name::<()>("session-toggled", &[]);
        assert!(press(&stage, gdk::Key::Escape));
        assert!(
            RootExt::focus(&window).is_some_and(|focus| focus == power()),
            "a mirror hands the focus back to the power button that opened the sheet"
        );
        stage.set_interactive(true);
        assert!(stage.prompt().grab_focus());

        window.destroy();
    }
}
