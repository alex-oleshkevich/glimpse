mod imp;

use gtk4::{glib, subclass::prelude::*};

glib::wrapper! {
    pub struct ShotPreferences(ObjectSubclass<imp::ShotPreferences>)
        @extends adw::PreferencesDialog, adw::Dialog, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for ShotPreferences {
    fn default() -> Self {
        Self::new()
    }
}

impl ShotPreferences {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn fields(&self) -> ShotPreferencesFields {
        let imp = self.imp();
        ShotPreferencesFields {
            page: imp.page.get(),
            read_only_banner: imp.read_only_banner.get(),
            saving: imp.saving.get(),
            behavior: imp.behavior.get(),
            annotations: imp.annotations.get(),
            keyboard: imp.keyboard.get(),
            folder: imp.folder.get(),
            choose_folder: imp.choose_folder.get(),
            filename: imp.filename.get(),
            example: imp.example.get(),
            editor: imp.editor.get(),
            delay: imp.delay.get(),
            cursor: imp.cursor.get(),
            flash: imp.flash.get(),
            notify: imp.notify.get(),
            color: imp.color.get(),
            red: imp.red.get(),
            yellow: imp.yellow.get(),
            green: imp.green.get(),
            blue: imp.blue.get(),
            white: imp.white.get(),
            width: imp.width.get(),
            strokes: imp.strokes.get(),
            copy_snippet: imp.copy_snippet.get(),
        }
    }
}

pub struct ShotPreferencesFields {
    pub page: adw::PreferencesPage,
    pub read_only_banner: adw::Banner,
    pub saving: adw::PreferencesGroup,
    pub behavior: adw::PreferencesGroup,
    pub annotations: adw::PreferencesGroup,
    pub keyboard: adw::PreferencesGroup,
    pub folder: adw::ActionRow,
    pub choose_folder: gtk4::Button,
    pub filename: adw::EntryRow,
    pub example: gtk4::Label,
    pub editor: adw::ComboRow,
    pub delay: adw::SpinRow,
    pub cursor: adw::SwitchRow,
    pub flash: adw::SwitchRow,
    pub notify: adw::SwitchRow,
    pub color: adw::ActionRow,
    pub red: gtk4::ToggleButton,
    pub yellow: gtk4::ToggleButton,
    pub green: gtk4::ToggleButton,
    pub blue: gtk4::ToggleButton,
    pub white: gtk4::ToggleButton,
    pub width: adw::ActionRow,
    pub strokes: adw::ToggleGroup,
    pub copy_snippet: gtk4::Button,
}
