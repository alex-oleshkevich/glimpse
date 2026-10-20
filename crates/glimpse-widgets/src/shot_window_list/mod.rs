mod imp;

use gettextrs::gettext;
use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

const TITLE_MAX: usize = 200;
const APP_ID_MAX: usize = 200;
const FALLBACK_ICON: &str = "application-x-executable-symbolic";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShotWindowEntry {
    pub id: u64,
    pub app_id: Option<String>,
    pub title: String,
    pub name: Option<String>,
    pub icon: Option<String>,
    pub this_workspace: bool,
    pub focused: bool,
    pub captured_when_picked: bool,
}

glib::wrapper! {
    pub struct ShotWindowList(ObjectSubclass<imp::ShotWindowList>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for ShotWindowList {
    fn default() -> Self {
        Self::new()
    }
}

impl ShotWindowList {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_windows(&self, windows: &[ShotWindowEntry]) {
        let imp = self.imp();
        if imp.windows.borrow().as_slice() == windows {
            return;
        }
        imp.windows.replace(windows.to_vec());
        self.render();
    }

    pub fn connect_window_picked<F: Fn(&Self, u64) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "window-picked",
            false,
            glib::closure_local!(move |list: Self, id: u64| f(&list, id)),
        )
    }

    fn render(&self) {
        let imp = self.imp();
        let windows = imp.windows.borrow();
        let (here, elsewhere): (Vec<_>, Vec<_>) =
            windows.iter().partition(|entry| entry.this_workspace);
        fill(&imp.this_rows, &here, self);
        fill(&imp.other_rows, &elsewhere, self);
        let here_shown = !here.is_empty();
        let elsewhere_shown = !elsewhere.is_empty();
        if imp.this_section.get_visible() != here_shown {
            imp.this_section.set_visible(here_shown);
        }
        if imp.this_rows.get_visible() != here_shown {
            imp.this_rows.set_visible(here_shown);
        }
        if imp.other_section.get_visible() != elsewhere_shown {
            imp.other_section.set_visible(elsewhere_shown);
        }
        if imp.other_rows.get_visible() != elsewhere_shown {
            imp.other_rows.set_visible(elsewhere_shown);
        }
    }

    fn focus_current(&self) {
        for rows in [&self.imp().this_rows, &self.imp().other_rows] {
            let mut child = rows.first_child();
            while let Some(widget) = child {
                if widget.has_css_class("shot-list__row--current") {
                    let _ = widget.grab_focus();
                    return;
                }
                child = widget.next_sibling();
            }
        }
    }
}

fn fill(rows: &gtk4::Box, entries: &[&ShotWindowEntry], list: &ShotWindowList) {
    crate::clear_children(rows);
    for entry in entries {
        rows.append(&row(entry, list));
    }
}

fn row(entry: &ShotWindowEntry, list: &ShotWindowList) -> gtk4::Button {
    let button = gtk4::Button::builder()
        .hexpand(true)
        .can_shrink(true)
        .build();
    button.add_css_class("flat");
    button.add_css_class("shot-list__row");
    if entry.focused {
        button.add_css_class("shot-list__row--current");
    }

    let icon = gtk4::Image::from_icon_name(&icon_name(entry));
    icon.set_pixel_size(32);
    icon.set_accessible_role(gtk4::AccessibleRole::Presentation);

    let app = gtk4::Label::builder()
        .label(primary(entry))
        .xalign(0.0)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .max_width_chars(40)
        .hexpand(true)
        .halign(gtk4::Align::Fill)
        .build();

    let title = gtk4::Label::builder()
        .label(capped(&entry.title, TITLE_MAX))
        .xalign(0.0)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .max_width_chars(40)
        .hexpand(true)
        .halign(gtk4::Align::Fill)
        .visible(shows_title(entry))
        .build();
    title.add_css_class("shot-list__title");

    let note = gtk4::Label::builder()
        .label(gettext("captured when picked"))
        .xalign(0.0)
        .visible(entry.captured_when_picked)
        .build();
    note.add_css_class("shot-list__note");

    let text = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    text.set_hexpand(true);
    text.append(&app);
    text.append(&title);
    text.append(&note);

    let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    line.append(&icon);
    line.append(&text);
    if entry.focused {
        let next = gtk4::Image::from_icon_name("go-next-symbolic");
        next.set_accessible_role(gtk4::AccessibleRole::Presentation);
        line.append(&next);
    }
    button.set_child(Some(&line));

    let id = entry.id;
    button.connect_clicked(glib::clone!(
        #[weak]
        list,
        move |_| list.emit_by_name::<()>("window-picked", &[&id])
    ));
    button
}

fn primary(entry: &ShotWindowEntry) -> String {
    if let Some(name) = entry.name.as_deref().and_then(non_empty) {
        return name;
    }
    if let Some(title) = non_empty(&entry.title) {
        return title;
    }
    entry
        .app_id
        .as_deref()
        .and_then(non_empty)
        .unwrap_or_default()
}

fn shows_title(entry: &ShotWindowEntry) -> bool {
    let title = capped(&entry.title, TITLE_MAX);
    !title.is_empty() && primary(entry) != title
}

fn non_empty(text: &str) -> Option<String> {
    let text = capped(text, TITLE_MAX);
    (!text.is_empty()).then_some(text)
}

fn icon_name(entry: &ShotWindowEntry) -> String {
    if let Some(icon) = entry.icon.as_deref().and_then(non_empty) {
        return icon;
    }
    icon_for(entry.app_id.as_deref())
}

fn capped_app(app_id: Option<&str>) -> Option<String> {
    let app = capped(app_id.unwrap_or_default(), APP_ID_MAX);
    (!app.is_empty()).then_some(app)
}

fn icon_for(app_id: Option<&str>) -> String {
    let Some(app) = capped_app(app_id) else {
        return FALLBACK_ICON.to_owned();
    };
    let Some(display) = gdk::Display::default() else {
        return app;
    };
    let theme = gtk4::IconTheme::for_display(&display);
    if theme.has_icon(&app) {
        return app;
    }
    let symbolic = format!("{app}-symbolic");
    if theme.has_icon(&symbolic) {
        symbolic
    } else {
        app
    }
}

fn capped(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}
