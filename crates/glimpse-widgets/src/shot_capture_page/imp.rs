use std::cell::{Cell, RefCell};

use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/shot_capture_page.ui")]
pub struct ShotCapturePage {
    #[template_child]
    pub targets: TemplateChild<gtk4::ListBox>,
    #[template_child]
    pub modes: TemplateChild<adw::ToggleGroup>,
    #[template_child]
    pub screen: TemplateChild<adw::ComboRow>,
    #[template_child]
    pub window: TemplateChild<adw::ComboRow>,
    #[template_child]
    pub delay: TemplateChild<adw::SpinRow>,
    #[template_child]
    pub cursor: TemplateChild<adw::SwitchRow>,
    #[template_child]
    pub take: TemplateChild<gtk4::Button>,
    #[template_child]
    pub menu: TemplateChild<gtk4::MenuButton>,

    pub outputs: RefCell<Vec<String>>,
    pub output_labels: RefCell<Vec<String>>,
    pub current_output: RefCell<String>,
    pub windows: RefCell<Vec<(u64, String)>>,
    pub quiet: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for ShotCapturePage {
    const NAME: &'static str = "ShotCapturePage";
    type Type = super::ShotCapturePage;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BinLayout>();
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for ShotCapturePage {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: std::sync::OnceLock<Vec<glib::subclass::Signal>> =
            std::sync::OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("mode-changed")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("output-selected")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("window-selected")
                    .param_types([u64::static_type()])
                    .build(),
                glib::subclass::Signal::builder("delay-changed")
                    .param_types([u32::static_type()])
                    .build(),
                glib::subclass::Signal::builder("cursor-changed")
                    .param_types([bool::static_type()])
                    .build(),
                glib::subclass::Signal::builder("take").build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        self.delay.set_digits(0);
        let adjustment = self.delay.adjustment();
        adjustment.set_step_increment(1.0);
        adjustment.set_page_increment(1.0);
        self.obj().wire();
    }

    fn dispose(&self) {
        self.dispose_template();
        while let Some(child) = self.obj().first_child() {
            child.unparent();
        }
    }
}

impl WidgetImpl for ShotCapturePage {}
