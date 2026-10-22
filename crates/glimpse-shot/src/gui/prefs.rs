use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use chrono::Local;
use gettextrs::gettext;
use glimpse_config::{ConfigError, Editor, FileTemplate, PaletteColor, ScreenshotConfig, Stroke};
use glimpse_shot::doc::naming::file_name;
use glimpse_widgets::{ShotPreferences, ShotPreferencesFields};
use gtk4::{gio, glib};

use super::result::home_relative;

const TABLE: &str = "screenshot";
const DEBOUNCE: Duration = Duration::from_millis(500);
const NIRI_BINDS: &str = "Print       { spawn \"glimpse-shot\" \"--screen\"; }\nCtrl+Print  { spawn \"glimpse-shot\" \"--window\"; }\nShift+Print { spawn \"glimpse-shot\" \"capture\" \"--copy\" \"--no-save\"; }\n";
struct Prefs {
    dialog: ShotPreferences,
    fields: ShotPreferencesFields,
    path: PathBuf,
    pending: RefCell<toml_edit::Table>,
    timer: RefCell<Option<glib::SourceId>>,
    frozen: Cell<bool>,
}

pub fn present(parent: &impl IsA<gtk4::Widget>, target: Option<&Path>) {
    build(target).present(Some(parent));
}

fn build(target: Option<&Path>) -> ShotPreferences {
    let path = target.map(Path::to_path_buf).unwrap_or_default();
    let loaded = glimpse_config::load(glimpse_config::Documents::Config);
    let locked = target.is_none() || !parses(&path) || !writable(&path);
    let invalid = loaded.is_err();
    let config = loaded.map(|config| config.screenshot).unwrap_or_default();

    let dialog = ShotPreferences::new();
    let fields = dialog.fields();
    let prefs = Rc::new(Prefs {
        dialog: dialog.clone(),
        fields,
        path,
        pending: RefCell::default(),
        timer: RefCell::default(),
        frozen: Cell::new(false),
    });
    prefs.build(&config);
    if locked || invalid {
        prefs.read_only();
    }
    let kept = RefCell::new(Some(prefs));
    dialog.connect_closed(move |_| {
        if let Some(prefs) = kept.take() {
            prefs.write_now();
        }
    });
    dialog
}

fn parses(path: &Path) -> bool {
    match std::fs::read_to_string(path) {
        Ok(text) => text.parse::<toml_edit::DocumentMut>().is_ok(),
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

fn writable(path: &Path) -> bool {
    match std::fs::OpenOptions::new().append(true).open(path) {
        Ok(_) => true,
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

fn stage(pending: &RefCell<toml_edit::Table>, key: &str, value: impl Into<toml_edit::Value>) {
    pending
        .borrow_mut()
        .insert(key, toml_edit::Item::Value(value.into()));
}

fn flush(path: &Path, pending: &RefCell<toml_edit::Table>) -> Result<(), ConfigError> {
    let values = pending.borrow().clone();
    if values.is_empty() {
        return Ok(());
    }
    glimpse_config::write_table(path, TABLE, &values, &[])?;
    pending.borrow_mut().clear();
    Ok(())
}

impl Prefs {
    fn build(self: &Rc<Self>, config: &ScreenshotConfig) {
        let fields = &self.fields;
        fields.folder.set_subtitle(&match &config.directory {
            Some(dir) => home_relative(dir, &glib::home_dir()),
            None => gettext("Pictures/Screenshots"),
        });
        let weak = Rc::downgrade(self);
        let folder = fields.folder.clone();
        fields.choose_folder.connect_clicked(move |_| {
            if let Some(prefs) = weak.upgrade() {
                prefs.choose_folder(&folder);
            }
        });

        fields.filename.set_text(
            config
                .filename
                .as_str()
                .strip_suffix(".png")
                .unwrap_or(config.filename.as_str()),
        );
        fields.example.set_text(
            &gettext("Saved as “{name}”")
                .replace("{name}", &file_name(&config.filename, Local::now())),
        );
        let weak = Rc::downgrade(self);
        let shown = fields.example.clone();
        fields.filename.connect_changed(move |entry| {
            let Some(prefs) = weak.upgrade() else {
                return;
            };
            let text = entry.text();
            if let Some(pattern) = text.strip_suffix(".png") {
                entry.set_text(pattern);
                return;
            }
            match FileTemplate::try_from(format!("{text}.png")) {
                Ok(template) => {
                    entry.remove_css_class("error");
                    shown.remove_css_class("error");
                    shown.set_text(
                        &gettext("Saved as “{name}”")
                            .replace("{name}", &file_name(&template, Local::now())),
                    );
                    prefs.change("filename", template.as_str());
                }
                Err(reason) => {
                    entry.add_css_class("error");
                    shown.add_css_class("error");
                    shown.set_text(&reason);
                    prefs.pending.borrow_mut().remove("filename");
                }
            }
        });

        fields.editor.set_selected(match config.editor {
            Editor::Overlay => 0,
            Editor::Window => 1,
        });
        let weak = Rc::downgrade(self);
        fields.editor.connect_selected_notify(move |row| {
            if let Some(prefs) = weak.upgrade() {
                prefs.change(
                    "editor",
                    if row.selected() == 1 {
                        "window"
                    } else {
                        "overlay"
                    },
                );
            }
        });

        fields.delay.set_value(f64::from(config.delay));
        fields.delay.set_digits(0);
        let adjustment = fields.delay.adjustment();
        adjustment.set_step_increment(1.0);
        adjustment.set_page_increment(1.0);
        let weak = Rc::downgrade(self);
        fields.delay.connect_value_notify(move |row| {
            if let Some(prefs) = weak.upgrade() {
                prefs.change("delay", row.value().round() as i64);
            }
        });

        for (row, key, active) in [
            (&fields.cursor, "cursor", config.cursor),
            (&fields.flash, "flash", config.flash),
            (&fields.notify, "notify", config.notify),
        ] {
            row.set_active(active);
            let weak = Rc::downgrade(self);
            row.connect_active_notify(move |row| {
                if let Some(prefs) = weak.upgrade() {
                    prefs.change(key, row.is_active());
                }
            });
        }

        for (value, name, button) in [
            (PaletteColor::Red, "red", &fields.red),
            (PaletteColor::Yellow, "yellow", &fields.yellow),
            (PaletteColor::Green, "green", &fields.green),
            (PaletteColor::Blue, "blue", &fields.blue),
            (PaletteColor::White, "white", &fields.white),
        ] {
            button.set_active(value == config.color);
            let weak = Rc::downgrade(self);
            button.connect_toggled(move |button| {
                if button.is_active()
                    && let Some(prefs) = weak.upgrade()
                {
                    prefs.change("color", name);
                }
            });
        }

        fields.strokes.set_active_name(Some(match config.stroke {
            Stroke::Thin => "thin",
            Stroke::Medium => "medium",
            Stroke::Thick => "thick",
        }));
        let weak = Rc::downgrade(self);
        fields.strokes.connect_active_name_notify(move |group| {
            if let (Some(prefs), Some(name)) = (weak.upgrade(), group.active_name()) {
                prefs.change("stroke", name.as_str());
            }
        });

        let weak = Rc::downgrade(self);
        fields.copy_snippet.connect_clicked(move |button| {
            button.clipboard().set_text(NIRI_BINDS);
            if let Some(prefs) = weak.upgrade() {
                prefs.toast(&gettext("Copied to clipboard"));
            }
        });
    }

    fn read_only(&self) {
        self.frozen.set(true);
        let banner = &self.fields.read_only_banner;
        let path = self.path.clone();
        let dialog = self.dialog.clone();
        banner.connect_button_clicked(move |_| {
            let root = dialog.root();
            let window = root
                .as_ref()
                .and_then(|root| root.downcast_ref::<gtk4::Window>());
            gtk4::FileLauncher::new(Some(&gio::File::for_path(&path))).launch(
                window,
                None::<&gio::Cancellable>,
                |opened| {
                    if let Err(error) = opened {
                        tracing::warn!("cannot open the configuration: {error}");
                    }
                },
            );
        });
        banner.set_revealed(true);
        for group in [
            &self.fields.saving,
            &self.fields.behavior,
            &self.fields.annotations,
        ] {
            group.set_sensitive(false);
        }
        self.pending.borrow_mut().clear();
        if let Some(timer) = self.timer.take() {
            timer.remove();
        }
    }

    fn change(self: &Rc<Self>, key: &str, value: impl Into<toml_edit::Value>) {
        if self.frozen.get() {
            return;
        }
        stage(&self.pending, key, value);
        if let Some(timer) = self.timer.take() {
            timer.remove();
        }
        let weak = Rc::downgrade(self);
        self.timer
            .replace(Some(glib::timeout_add_local_once(DEBOUNCE, move || {
                if let Some(prefs) = weak.upgrade() {
                    prefs.timer.take();
                    prefs.write_now();
                }
            })));
    }

    fn write_now(&self) {
        if let Some(timer) = self.timer.take() {
            timer.remove();
        }
        match flush(&self.path, &self.pending) {
            Ok(()) => {}
            Err(error @ ConfigError::Parse { .. }) => {
                tracing::warn!("{error}");
                self.read_only();
            }
            Err(error) => {
                tracing::warn!("{error}");
                self.toast(&error.to_string());
            }
        }
    }

    fn choose_folder(self: &Rc<Self>, row: &adw::ActionRow) {
        let dialog = gtk4::FileDialog::builder()
            .title(gettext("Choose Folder"))
            .modal(true)
            .build();
        let root = self.dialog.root();
        let window = root
            .as_ref()
            .and_then(|root| root.downcast_ref::<gtk4::Window>());
        let weak = Rc::downgrade(self);
        let row = row.clone();
        dialog.select_folder(window, None::<&gio::Cancellable>, move |chosen| {
            let (Some(prefs), Some(folder)) =
                (weak.upgrade(), chosen.ok().and_then(|file| file.path()))
            else {
                return;
            };
            row.set_subtitle(&home_relative(&folder, &glib::home_dir()));
            prefs.change("directory", folder.to_string_lossy().as_ref());
        });
    }

    fn toast(&self, text: &str) {
        let toast = adw::Toast::new(text);
        toast.set_use_markup(false);
        self.dialog.add_toast(toast);
    }
}

#[cfg(test)]
mod tests;
