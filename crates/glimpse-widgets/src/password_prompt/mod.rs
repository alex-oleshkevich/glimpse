mod imp;

use gtk4::{accessible, glib, glib::translate::ToGlibPtr, prelude::*, subclass::prelude::*};
use std::ffi::CStr;
use std::time::Duration;
use zeroize::Zeroizing;

use crate::{set_css_class, set_text_capped};

const USER_MAX_CHARS: usize = 64;
const MESSAGE_MAX_CHARS: usize = 120;
const ERROR: &str = "password-prompt__message--error";
const SHAKE: &str = "password-prompt--shake";
const SHAKE_FOR: Duration = Duration::from_millis(400);

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub enum MessageKind {
    #[default]
    Info,
    Error,
}

glib::wrapper! {
    pub struct PasswordPrompt(ObjectSubclass<imp::PasswordPrompt>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for PasswordPrompt {
    fn default() -> Self {
        Self::new()
    }
}

impl PasswordPrompt {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_user(&self, user: Option<&str>) {
        set_text_capped(&self.imp().user, user, USER_MAX_CHARS);
    }

    pub fn set_busy(&self, busy: bool) {
        let imp = self.imp();
        if imp.busy.replace(busy) == busy {
            return;
        }
        if busy {
            imp.conceal();
        }
        imp.spinner.set_visible(busy);
        imp.spinner.set_spinning(busy);
        imp.entry.set_show_peek_icon(!busy);
        imp.entry.update_state(&[accessible::State::Busy(busy)]);
        imp.sync_editable();
    }

    pub fn set_message(&self, text: Option<&str>, kind: MessageKind) {
        let message = &self.imp().message;
        let text = glimpse_utils::clean(text.unwrap_or_default(), MESSAGE_MAX_CHARS);
        let error = kind == MessageKind::Error && !text.is_empty();
        if message.text().as_str() != text {
            message.set_text(&text);
            if error {
                self.announce(&text, gtk4::AccessibleAnnouncementPriority::High);
            }
        }
        if message.has_css_class(ERROR) != error {
            set_css_class(&**message, ERROR, error);
        }
    }

    pub fn set_caps_lock(&self, on: bool) {
        let caps = &self.imp().caps;
        if caps.is_child_visible() != on {
            caps.set_child_visible(on);
        }
    }

    pub fn set_available(&self, available: bool) {
        let imp = self.imp();
        let unavailable = !available;
        if imp.unavailable.replace(unavailable) == unavailable {
            return;
        }
        if unavailable {
            imp.release();
        }
        imp.entry_row.set_child_visible(available);
        imp.sync_editable();
    }

    pub fn set_interactive(&self, interactive: bool) {
        let imp = self.imp();
        let mirrored = !interactive;
        if imp.mirrored.replace(mirrored) == mirrored {
            return;
        }
        if mirrored {
            imp.release();
        }
        imp.entry.set_can_focus(interactive);
        imp.entry.set_can_target(interactive);
        imp.sync_editable();
    }

    pub fn shake(&self) {
        let imp = self.imp();
        if imp.shaking.replace(true) {
            return;
        }
        self.add_css_class(SHAKE);
        let prompt = self.downgrade();
        glib::timeout_add_local_once(SHAKE_FOR, move || {
            if let Some(prompt) = prompt.upgrade() {
                prompt.remove_css_class(SHAKE);
                prompt.imp().shaking.set(false);
            }
        });
    }

    pub fn take_text(&self) -> Zeroizing<String> {
        let imp = self.imp();
        let editable: &gtk4::Editable = imp.entry.upcast_ref();
        let raw = unsafe { gtk4::ffi::gtk_editable_get_text(editable.to_glib_none().0) };
        let text = match raw.is_null() {
            true => Zeroizing::new(String::new()),
            false => Zeroizing::new(
                unsafe { CStr::from_ptr(raw) }
                    .to_string_lossy()
                    .into_owned(),
            ),
        };
        imp.entry.set_text("");
        imp.conceal();
        text
    }

    pub fn connect_submitted<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "submitted",
            false,
            glib::closure_local!(move |prompt: Self| f(&prompt)),
        )
    }

    pub fn connect_edited<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "edited",
            false,
            glib::closure_local!(move |prompt: Self| f(&prompt)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static CRITICALS: AtomicUsize = AtomicUsize::new(0);

    fn submit(prompt: &PasswordPrompt, text: &str) {
        let entry = &prompt.imp().entry;
        entry.set_text(text);
        entry.emit_activate();
    }

    fn focused_in(window: &gtk4::Window, prompt: &PasswordPrompt) -> bool {
        RootExt::focus(window).is_some_and(|focus| focus.is_ancestor(&*prompt.imp().entry))
    }

    fn peeking(prompt: &PasswordPrompt) -> bool {
        prompt
            .imp()
            .text()
            .is_some_and(|text| text.property::<bool>("visibility"))
    }

    #[test]
    #[ignore = "needs a display"]
    fn password_prompt_states() {
        glib::log_set_writer_func(|level, fields| {
            if level == glib::LogLevel::Critical {
                CRITICALS.fetch_add(1, Ordering::Relaxed);
            }
            glib::log_writer_default(level, fields)
        });
        if gtk4::init().is_err() {
            return;
        }
        crate::register_resources().expect("resources");

        let prompt = PasswordPrompt::new();
        let window = gtk4::Window::new();
        window.set_child(Some(&prompt));
        let imp = prompt.imp();
        let submitted = Rc::new(Cell::new(0));
        prompt.connect_submitted({
            let submitted = Rc::clone(&submitted);
            move |_| submitted.set(submitted.get() + 1)
        });
        let edited = Rc::new(Cell::new(0));
        prompt.connect_edited({
            let edited = Rc::clone(&edited);
            move |_| edited.set(edited.get() + 1)
        });

        submit(&prompt, "");
        assert_eq!(submitted.get(), 0, "an empty entry submits nothing");
        submit(&prompt, "hunter2");
        assert_eq!(submitted.get(), 1, "a filled entry submits once");
        assert!(edited.get() > 0, "typing reaches the typed edited wrapper");

        imp.text().expect("delegate").set_visibility(true);
        assert_eq!(prompt.take_text().as_str(), "hunter2");
        assert_eq!(imp.entry.text().as_str(), "", "take_text clears the entry");
        assert!(!peeking(&prompt), "the next attempt is typed concealed");
        assert_eq!(
            prompt.take_text().as_str(),
            "",
            "an empty entry reads empty"
        );

        assert!(
            prompt.grab_focus(),
            "the prompt forwards focus to its entry"
        );
        assert!(focused_in(&window, &prompt));
        imp.text().expect("delegate").set_visibility(true);
        prompt.set_busy(true);
        assert!(!imp.entry.is_editable(), "a busy prompt refuses input");
        assert!(focused_in(&window, &prompt), "verifying keeps the focus");
        assert!(!peeking(&prompt), "the peek is undone before its icon goes");
        assert_eq!(
            CRITICALS.load(Ordering::Relaxed),
            0,
            "hiding the peek icon while peeking logs no GTK critical"
        );
        assert!(imp.spinner.get_visible() && imp.spinner.is_spinning());
        assert!(
            !imp.entry.shows_peek_icon(),
            "the spinner takes the peek slot"
        );
        imp.entry.set_text("again");
        imp.entry.emit_activate();
        assert_eq!(submitted.get(), 1, "a busy prompt submits nothing");
        prompt.set_busy(false);
        assert!(imp.entry.is_editable());
        assert!(!imp.spinner.get_visible());
        assert!(imp.entry.shows_peek_icon());

        prompt.set_interactive(false);
        assert!(!imp.entry.can_focus() && !imp.entry.is_editable());
        assert_eq!(imp.entry.text().as_str(), "", "a mirror holds no text");
        assert!(!focused_in(&window, &prompt), "a mirror drops the focus");
        imp.entry.set_text("again");
        imp.entry.emit_activate();
        assert_eq!(submitted.get(), 1, "a mirrored prompt submits nothing");
        prompt.set_busy(true);
        prompt.set_busy(false);
        assert!(
            !imp.entry.is_editable(),
            "ending a verification does not make a mirrored entry editable"
        );
        prompt.set_interactive(true);
        assert!(imp.entry.can_focus() && imp.entry.is_editable());

        imp.entry.set_text("half typed");
        assert!(prompt.grab_focus());
        prompt.set_available(false);
        assert!(!imp.entry_row.is_child_visible());
        assert!(
            !imp.entry.is_editable(),
            "a hidden entry cannot be typed into"
        );
        assert_eq!(
            imp.entry.text().as_str(),
            "",
            "text typed before the probe failed is gone"
        );
        assert!(
            !focused_in(&window, &prompt),
            "a hidden entry keeps no focus"
        );
        imp.entry.emit_activate();
        assert_eq!(submitted.get(), 1, "an unavailable prompt submits nothing");
        prompt.set_available(true);
        assert!(imp.entry_row.is_child_visible() && imp.entry.is_editable());
        submit(&prompt, "glimpse");
        assert_eq!(submitted.get(), 2);
        prompt.take_text();

        let (_, empty, _, _) = prompt.measure(gtk4::Orientation::Vertical, -1);
        prompt.set_message(Some("Wrong password"), MessageKind::Error);
        assert_eq!(imp.message.text().as_str(), "Wrong password");
        assert!(imp.message.has_css_class(ERROR));
        assert!(imp.message.get_visible(), "the message line never hides");
        let (_, filled, _, _) = prompt.measure(gtk4::Orientation::Vertical, -1);
        assert_eq!(empty, filled, "a one-line message does not move the layout");
        let capped = "word ".repeat(MESSAGE_MAX_CHARS / 5);
        prompt.set_message(Some(capped.trim_end()), MessageKind::Error);
        let (_, most, _, _) = prompt.measure(gtk4::Orientation::Vertical, -1);
        assert!(
            most > filled,
            "a wrapping message is the one allowed growth"
        );
        prompt.set_message(Some(&"word ".repeat(200)), MessageKind::Error);
        let (_, long, _, _) = prompt.measure(gtk4::Orientation::Vertical, -1);
        assert_eq!(long, most, "a long message grows no further than the cap");
        prompt.set_message(Some(&"word\n".repeat(200)), MessageKind::Error);
        let (_, broken, _, _) = prompt.measure(gtk4::Orientation::Vertical, -1);
        assert_eq!(broken, most, "embedded newlines are one paragraph");
        prompt.set_message(None, MessageKind::Error);
        assert!(
            !imp.message.has_css_class(ERROR),
            "an empty line is never red"
        );
        assert!(imp.message.get_visible());

        let (_, before, _, _) = prompt.measure(gtk4::Orientation::Vertical, -1);
        prompt.set_caps_lock(true);
        assert!(imp.caps.is_child_visible());
        let (_, after, _, _) = prompt.measure(gtk4::Orientation::Vertical, -1);
        assert_eq!(before, after, "the Caps Lock note keeps its space");
        prompt.set_caps_lock(false);
        assert!(!imp.caps.is_child_visible());

        prompt.set_user(Some(&"é".repeat(200)));
        assert_eq!(imp.user.text().chars().count(), USER_MAX_CHARS);
        prompt.set_message(Some(&"ж".repeat(600)), MessageKind::Info);
        assert_eq!(imp.message.text().chars().count(), MESSAGE_MAX_CHARS + 1);

        window.destroy();
    }
}
