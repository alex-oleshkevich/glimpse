use adw::subclass::prelude::*;
use gtk4::{CompositeTemplate, TemplateChild, glib};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/shot_preferences.ui")]
pub struct ShotPreferences {
    #[template_child]
    pub page: TemplateChild<adw::PreferencesPage>,
    #[template_child]
    pub read_only_banner: TemplateChild<adw::Banner>,
    #[template_child]
    pub saving: TemplateChild<adw::PreferencesGroup>,
    #[template_child]
    pub behavior: TemplateChild<adw::PreferencesGroup>,
    #[template_child]
    pub annotations: TemplateChild<adw::PreferencesGroup>,
    #[template_child]
    pub keyboard: TemplateChild<adw::PreferencesGroup>,
    #[template_child]
    pub folder: TemplateChild<adw::ActionRow>,
    #[template_child]
    pub choose_folder: TemplateChild<gtk4::Button>,
    #[template_child]
    pub filename: TemplateChild<adw::EntryRow>,
    #[template_child]
    pub example: TemplateChild<gtk4::Label>,
    #[template_child]
    pub editor: TemplateChild<adw::ComboRow>,
    #[template_child]
    pub delay: TemplateChild<adw::SpinRow>,
    #[template_child]
    pub cursor: TemplateChild<adw::SwitchRow>,
    #[template_child]
    pub flash: TemplateChild<adw::SwitchRow>,
    #[template_child]
    pub notify: TemplateChild<adw::SwitchRow>,
    #[template_child]
    pub color: TemplateChild<adw::ActionRow>,
    #[template_child]
    pub red: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub yellow: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub green: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub blue: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub white: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub width: TemplateChild<adw::ActionRow>,
    #[template_child]
    pub strokes: TemplateChild<adw::ToggleGroup>,
    #[template_child]
    pub copy_snippet: TemplateChild<gtk4::Button>,
}

#[glib::object_subclass]
impl ObjectSubclass for ShotPreferences {
    const NAME: &'static str = "ShotPreferences";
    type Type = super::ShotPreferences;
    type ParentType = adw::PreferencesDialog;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for ShotPreferences {}
impl WidgetImpl for ShotPreferences {}
impl AdwDialogImpl for ShotPreferences {}
impl PreferencesDialogImpl for ShotPreferences {}
