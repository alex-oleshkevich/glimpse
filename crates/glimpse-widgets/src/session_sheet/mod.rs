mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::SessionActionState;

glib::wrapper! {
    pub struct SessionSheet(ObjectSubclass<imp::SessionSheet>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for SessionSheet {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionSheet {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_action(&self, action: &str, state: &SessionActionState) {
        let imp = self.imp();
        let row = match action {
            crate::SUSPEND => &imp.suspend,
            crate::REBOOT => &imp.reboot,
            crate::POWER_OFF => &imp.power_off,
            other => {
                tracing::warn!(action = other, "unknown session action");
                return;
            }
        };
        if row.get_visible() == state.visible
            && row.is_sensitive() == state.enabled
            && row.subtitle().as_deref() == state.subtitle.as_deref()
        {
            return;
        }
        row.set_visible(state.visible);
        row.set_sensitive(state.enabled);
        row.set_subtitle(state.subtitle.as_deref());
        imp.close_confirm_if_unavailable(action, state);
    }

    pub fn set_error(&self, text: Option<&str>) {
        let imp = self.imp();
        if !imp.revealer.reveals_child() {
            return;
        }
        let error = &imp.error;
        let flat = glimpse_utils::clean(text.unwrap_or_default(), crate::TEXT_MAX_CHARS);
        if error.text().as_str() != flat {
            error.set_text(&flat);
            error.set_visible(!flat.is_empty());
        }
        if !flat.is_empty() {
            imp.return_to_menu_and_focus();
            self.announce(&flat, gtk4::AccessibleAnnouncementPriority::High);
        }
    }

    pub fn toggle(&self) {
        let imp = self.imp();
        if !imp.revealer.reveals_child() && !imp.has_actions() {
            return;
        }
        crate::drawer::toggle(&imp.revealer);
    }

    pub fn close(&self) {
        crate::drawer::set(&self.imp().revealer, false);
    }

    pub fn is_open(&self) -> bool {
        self.imp().revealer.reveals_child()
    }

    pub fn has_actions(&self) -> bool {
        self.imp().has_actions()
    }

    pub fn connect_action_requested<F: Fn(&Self, &str) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "action-requested",
            false,
            glib::closure_local!(move |sheet: Self, action: String| f(&sheet, &action)),
        )
    }

    pub fn connect_closed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "closed",
            false,
            glib::closure_local!(move |sheet: Self| f(&sheet)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SessionActionState;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    fn state(visible: bool, enabled: bool, subtitle: Option<&str>) -> SessionActionState {
        SessionActionState {
            visible,
            enabled,
            subtitle: subtitle.map(str::to_owned),
        }
    }

    fn exact_focus(window: &gtk4::Window, widget: &impl IsA<gtk4::Widget>) -> bool {
        RootExt::focus(window).is_some_and(|focus| &focus == widget.upcast_ref::<gtk4::Widget>())
    }

    #[test]
    #[ignore = "needs a display"]
    fn session_sheet_states() {
        if gtk4::init().is_err() {
            return;
        }
        crate::register_resources().expect("resources");

        let sheet = SessionSheet::new();
        let outside = gtk4::Button::new();
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        container.append(&outside);
        container.append(&sheet);
        let window = gtk4::Window::new();
        window.set_child(Some(&container));
        let imp = sheet.imp();

        let (_, closed_height, _, _) = sheet.measure(gtk4::Orientation::Vertical, -1);
        assert_eq!(closed_height, 0, "a closed sheet paints no strip");

        let requested = Rc::new(RefCell::new(Vec::new()));
        sheet.connect_action_requested({
            let requested = Rc::clone(&requested);
            move |_, action| requested.borrow_mut().push(action.to_owned())
        });
        let closed = Rc::new(Cell::new(0));
        sheet.connect_closed({
            let closed = Rc::clone(&closed);
            move |_| closed.set(closed.get() + 1)
        });

        assert!(
            !sheet.has_actions(),
            "no row is visible until set_action shows one"
        );
        sheet.toggle();
        assert!(!sheet.is_open(), "toggle refuses to open an empty sheet");

        sheet.set_action(crate::SUSPEND, &state(true, true, None));
        sheet.set_action(crate::REBOOT, &state(true, true, None));
        sheet.set_action(crate::POWER_OFF, &state(true, true, None));
        assert!(sheet.has_actions());

        outside.grab_focus();
        assert!(
            exact_focus(&window, &outside),
            "the outside widget holds focus before the closed-sheet check"
        );
        sheet.set_error(Some("Restart failed"));
        assert!(
            exact_focus(&window, &outside),
            "set_error on a closed sheet steals no focus"
        );
        assert!(
            !imp.error.get_visible(),
            "set_error on a closed sheet shows nothing"
        );

        sheet.toggle();
        assert!(sheet.is_open(), "a closed sheet opens");
        assert!(
            exact_focus(&window, &*imp.suspend),
            "opening focuses exactly the suspend row"
        );
        sheet.toggle();
        assert!(!sheet.is_open(), "an open sheet closes");
        assert_eq!(closed.get(), 1, "closing fires the typed signal once");

        sheet.toggle();
        assert!(sheet.is_open());
        sheet.close();
        assert!(!sheet.is_open(), "close() closes an open sheet");
        assert_eq!(closed.get(), 2, "close() fires the typed signal too");

        sheet.set_action(crate::SUSPEND, &state(false, true, None));
        sheet.toggle();
        assert!(
            exact_focus(&window, &*imp.reboot),
            "suspend hidden: opening focuses exactly the reboot row"
        );
        sheet.toggle();
        assert_eq!(closed.get(), 3);
        sheet.set_action(crate::SUSPEND, &state(true, true, None));

        sheet.toggle();
        imp.suspend.emit_clicked();
        assert!(
            requested.borrow().is_empty(),
            "the suspend row alone emits nothing"
        );
        assert_eq!(imp.stack.visible_child_name().as_deref(), Some("suspend"));
        assert!(exact_focus(&window, &*imp.suspend_cancel));
        imp.suspend_cancel.emit_clicked();
        assert_eq!(imp.stack.visible_child_name().as_deref(), Some("menu"));
        assert!(exact_focus(&window, &*imp.suspend));
        assert!(requested.borrow().is_empty(), "cancel emits nothing");
        imp.suspend.emit_clicked();
        imp.suspend_confirm.emit_clicked();
        assert_eq!(*requested.borrow(), ["suspend"]);
        sheet.set_action(crate::SUSPEND, &state(true, false, None));
        assert_eq!(
            imp.stack.visible_child_name().as_deref(),
            Some("menu"),
            "disabling suspend while its confirm page is open returns to the menu"
        );
        sheet.set_action(crate::SUSPEND, &state(true, true, None));

        imp.reboot.emit_clicked();
        assert!(
            requested.borrow().len() == 1,
            "the restart row alone emits nothing"
        );
        assert_eq!(imp.stack.visible_child_name().as_deref(), Some("reboot"));
        assert!(
            exact_focus(&window, &*imp.reboot_cancel),
            "the confirm page focuses its Cancel button"
        );

        imp.reboot_cancel.emit_clicked();
        assert_eq!(
            imp.stack.visible_child_name().as_deref(),
            Some("menu"),
            "cancel returns to the menu"
        );
        assert!(
            exact_focus(&window, &*imp.reboot),
            "cancel returns focus to the row that opened the page"
        );
        assert_eq!(requested.borrow().len(), 1, "cancel emits nothing");

        imp.reboot.emit_clicked();
        assert_eq!(imp.stack.visible_child_name().as_deref(), Some("reboot"));
        imp.reboot_confirm.emit_clicked();
        assert_eq!(*requested.borrow(), ["suspend", "reboot"]);

        imp.power_off.emit_clicked();
        assert_eq!(imp.stack.visible_child_name().as_deref(), Some("power-off"));
        assert!(exact_focus(&window, &*imp.power_off_cancel));
        imp.power_off_cancel.emit_clicked();
        assert_eq!(
            imp.stack.visible_child_name().as_deref(),
            Some("menu"),
            "cancel returns to the menu"
        );
        assert!(exact_focus(&window, &*imp.power_off));
        assert_eq!(requested.borrow().len(), 2, "cancel emits nothing");

        imp.power_off.emit_clicked();
        imp.power_off_confirm.emit_clicked();
        assert_eq!(*requested.borrow(), ["suspend", "reboot", "power-off"]);
        assert_eq!(
            imp.stack.visible_child_name().as_deref(),
            Some("power-off"),
            "confirming does not itself return to the menu"
        );

        sheet.set_action(crate::POWER_OFF, &state(true, false, None));
        assert_eq!(
            imp.stack.visible_child_name().as_deref(),
            Some("menu"),
            "disabling the open confirm page's action returns to the menu"
        );
        imp.power_off_confirm.emit_clicked();
        assert_eq!(
            requested.borrow().len(),
            3,
            "a confirm button whose action is no longer available cannot emit"
        );
        sheet.set_action(crate::POWER_OFF, &state(true, true, None));

        sheet.set_action(crate::SUSPEND, &state(false, true, None));
        assert!(
            !imp.suspend.get_visible(),
            "a hidden action's row is invisible"
        );
        imp.suspend.set_visible(true);

        let hostile_subtitle = "ж".repeat(300);
        sheet.set_action(crate::SUSPEND, &state(true, false, Some(&hostile_subtitle)));
        assert!(!imp.suspend.is_sensitive());
        assert_eq!(
            imp.suspend.subtitle().map(|s| s.chars().count()),
            Some(crate::TEXT_MAX_CHARS),
            "a hostile subtitle is capped by characters"
        );
        imp.suspend.emit_clicked();
        assert_eq!(requested.borrow().len(), 3, "a disabled row cannot emit");

        sheet.set_action("unknown", &state(true, true, None));

        imp.reboot.emit_clicked();
        assert_eq!(imp.stack.visible_child_name().as_deref(), Some("reboot"));
        sheet.set_error(Some("Restart failed"));
        assert_eq!(
            imp.stack.visible_child_name().as_deref(),
            Some("menu"),
            "an error confirmed from a confirm page returns to the menu"
        );
        assert!(imp.error.get_visible());
        assert!(
            exact_focus(&window, &*imp.reboot),
            "the error moves focus to the first visible, enabled row"
        );

        imp.reboot.emit_clicked();
        assert_eq!(imp.stack.visible_child_name().as_deref(), Some("reboot"));
        sheet.set_error(None);
        assert_eq!(
            imp.stack.visible_child_name().as_deref(),
            Some("reboot"),
            "set_error(None) changes no page"
        );
        imp.reboot_cancel.emit_clicked();

        sheet.set_error(Some("Couldn't complete that: the request was refused"));
        assert!(imp.error.get_visible());
        assert_eq!(
            imp.error.text().as_str(),
            "Couldn't complete that: the request was refused"
        );

        let hostile_error = "ошибка\nошибка\n".repeat(60);
        sheet.set_error(Some(&hostile_error));
        assert_eq!(
            imp.error.text().chars().count(),
            crate::TEXT_MAX_CHARS + 1,
            "a hostile error is capped by characters, with a trailing ellipsis"
        );
        assert!(
            !imp.error.text().contains('\n'),
            "the error line is flattened to one line"
        );

        sheet.toggle();
        assert_eq!(closed.get(), 4);
        assert!(!imp.error.get_visible(), "closing clears the error");
        assert_eq!(
            imp.stack.visible_child_name().as_deref(),
            Some("menu"),
            "closing returns the page to the menu"
        );

        window.destroy();
    }
}
