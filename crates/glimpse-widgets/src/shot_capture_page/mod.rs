mod imp;

use adw::prelude::*;
use gettextrs::gettext;
use gtk4::{glib, subclass::prelude::*};

const TITLE_MAX: usize = 200;
const DELAY_MAX: u32 = 60;

glib::wrapper! {
    pub struct ShotCapturePage(ObjectSubclass<imp::ShotCapturePage>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for ShotCapturePage {
    fn default() -> Self {
        Self::new()
    }
}

impl ShotCapturePage {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_mode(&self, mode: &str) {
        if !matches!(mode, "screen" | "window" | "region") {
            return;
        }
        let imp = self.imp();
        if imp.modes.active_name().as_deref() == Some(mode) {
            return;
        }
        imp.quiet.set(true);
        imp.modes.set_active_name(Some(mode));
        imp.quiet.set(false);
    }

    pub fn set_outputs(&self, outputs: &[String], current: &str) {
        let labeled: Vec<(String, String)> = outputs
            .iter()
            .map(|name| (name.clone(), name.clone()))
            .collect();
        self.set_outputs_labeled(&labeled, current);
    }

    pub fn set_outputs_labeled(&self, outputs: &[(String, String)], current: &str) {
        let imp = self.imp();
        let names: Vec<String> = outputs.iter().map(|(name, _)| name.clone()).collect();
        let shown: Vec<String> = outputs
            .iter()
            .map(|(_, label)| label.chars().take(TITLE_MAX).collect())
            .collect();
        if imp.outputs.borrow().as_slice() == names
            && imp.output_labels.borrow().as_slice() == shown
            && imp.current_output.borrow().as_str() == current
        {
            return;
        }
        let previous = self.selected_output();
        imp.outputs.replace(names);
        imp.output_labels.replace(shown.clone());
        imp.current_output.replace(current.to_owned());
        let mut labels = Vec::with_capacity(outputs.len() + 1);
        labels.push(gettext("Current Screen"));
        labels.extend(shown);
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let list = gtk4::StringList::new(&refs);
        let selected = match outputs.iter().position(|(name, _)| name == &previous) {
            Some(index) if previous != "current" => (index + 1) as u32,
            _ => 0,
        };
        imp.quiet.set(true);
        imp.screen.set_model(Some(&list));
        imp.screen.set_selected(selected);
        imp.quiet.set(false);
        self.sync_rows();
    }

    pub fn set_windows(&self, windows: &[(u64, String)]) {
        let imp = self.imp();
        if imp.windows.borrow().as_slice() == windows {
            return;
        }
        let previous = self.selected_window();
        imp.windows.replace(windows.to_vec());
        let titles: Vec<String> = windows
            .iter()
            .map(|(_, title)| title.chars().take(TITLE_MAX).collect())
            .collect();
        let refs: Vec<&str> = titles.iter().map(String::as_str).collect();
        let list = gtk4::StringList::new(&refs);
        let selected = previous
            .and_then(|id| windows.iter().position(|(window, _)| *window == id))
            .unwrap_or(0) as u32;
        imp.quiet.set(true);
        imp.window.set_model(Some(&list));
        if !windows.is_empty() {
            imp.window.set_selected(selected);
        }
        imp.quiet.set(false);
        self.sync_rows();
    }

    pub fn selected_window(&self) -> Option<u64> {
        let imp = self.imp();
        let index = imp.window.selected();
        if index == gtk4::INVALID_LIST_POSITION {
            return None;
        }
        imp.windows.borrow().get(index as usize).map(|(id, _)| *id)
    }

    pub fn selected_output(&self) -> String {
        let imp = self.imp();
        output_name(&imp.outputs.borrow(), imp.screen.selected())
            .unwrap_or_else(|| "current".to_owned())
    }

    pub fn set_menu_model(&self, model: Option<&gtk4::gio::MenuModel>) {
        set_menu(&self.imp().menu, model);
    }

    pub fn set_delay(&self, seconds: u32) {
        let seconds = seconds.min(DELAY_MAX);
        let imp = self.imp();
        if imp.delay.value().round() as u32 == seconds {
            return;
        }
        imp.quiet.set(true);
        imp.delay.set_value(f64::from(seconds));
        imp.quiet.set(false);
    }

    pub fn set_cursor(&self, shown: bool) {
        let imp = self.imp();
        if imp.cursor.is_active() == shown {
            return;
        }
        imp.quiet.set(true);
        imp.cursor.set_active(shown);
        imp.quiet.set(false);
    }

    pub fn connect_mode_changed<F: Fn(&Self, String) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "mode-changed",
            false,
            glib::closure_local!(move |page: Self, mode: String| f(&page, mode)),
        )
    }

    pub fn connect_output_selected<F: Fn(&Self, String) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "output-selected",
            false,
            glib::closure_local!(move |page: Self, output: String| f(&page, output)),
        )
    }

    pub fn connect_window_selected<F: Fn(&Self, u64) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "window-selected",
            false,
            glib::closure_local!(move |page: Self, id: u64| f(&page, id)),
        )
    }

    pub fn connect_delay_changed<F: Fn(&Self, u32) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "delay-changed",
            false,
            glib::closure_local!(move |page: Self, seconds: u32| f(&page, seconds)),
        )
    }

    pub fn connect_cursor_changed<F: Fn(&Self, bool) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "cursor-changed",
            false,
            glib::closure_local!(move |page: Self, shown: bool| f(&page, shown)),
        )
    }

    pub fn connect_take<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "take",
            false,
            glib::closure_local!(move |page: Self| f(&page)),
        )
    }

    fn wire(&self) {
        let imp = self.imp();
        imp.modes.connect_active_name_notify(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |_| {
                page.sync_rows();
                if page.imp().quiet.get() {
                    return;
                }
                if let Some(mode) = page.imp().modes.active_name() {
                    page.emit_by_name::<()>("mode-changed", &[&mode.to_string()]);
                }
            }
        ));
        imp.screen.connect_selected_notify(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |_| {
                if page.imp().quiet.get() {
                    return;
                }
                let Some(name) =
                    output_name(&page.imp().outputs.borrow(), page.imp().screen.selected())
                else {
                    return;
                };
                page.emit_by_name::<()>("output-selected", &[&name]);
            }
        ));
        imp.window.connect_selected_notify(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |_| {
                if page.imp().quiet.get() {
                    return;
                }
                let index = page.imp().window.selected() as usize;
                let Some(id) = page.imp().windows.borrow().get(index).map(|(id, _)| *id) else {
                    return;
                };
                page.emit_by_name::<()>("window-selected", &[&id]);
            }
        ));
        imp.delay.connect_value_notify(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |row| {
                if page.imp().quiet.get() {
                    return;
                }
                let seconds = row.value().round().clamp(0.0, f64::from(DELAY_MAX)) as u32;
                page.emit_by_name::<()>("delay-changed", &[&seconds]);
            }
        ));
        imp.cursor.connect_active_notify(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |row| {
                if page.imp().quiet.get() {
                    return;
                }
                let shown = row.is_active();
                page.emit_by_name::<()>("cursor-changed", &[&shown]);
            }
        ));
        imp.take.connect_clicked(glib::clone!(
            #[weak(rename_to = page)]
            self,
            move |_| page.emit_by_name::<()>("take", &[])
        ));
    }

    fn sync_rows(&self) {
        let imp = self.imp();
        let mode = imp.modes.active_name().unwrap_or_default();
        let screen = mode.as_str() == "screen" && imp.outputs.borrow().len() >= 2;
        let window = mode.as_str() == "window" && !imp.windows.borrow().is_empty();
        if imp.screen.get_visible() != screen {
            imp.screen.set_visible(screen);
        }
        if imp.window.get_visible() != window {
            imp.window.set_visible(window);
        }
        if imp.targets.get_visible() != (screen || window) {
            imp.targets.set_visible(screen || window);
        }
    }
}

fn output_name(outputs: &[String], index: u32) -> Option<String> {
    if index == gtk4::INVALID_LIST_POSITION {
        return None;
    }
    if index == 0 {
        return Some("current".to_owned());
    }
    outputs.get(index as usize - 1).cloned()
}

fn set_menu(button: &gtk4::MenuButton, model: Option<&gtk4::gio::MenuModel>) {
    let same = match (button.menu_model(), model) {
        (Some(current), Some(next)) => &current == next,
        (None, None) => true,
        _ => false,
    };
    if same {
        return;
    }
    button.set_menu_model(model);
    let visible = model.is_some();
    if button.get_visible() != visible {
        button.set_visible(visible);
    }
}
