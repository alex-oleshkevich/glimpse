use std::cell::Cell;
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/shot_toolbar.ui")]
pub struct ShotToolbar {
    #[template_child]
    pub handle: TemplateChild<gtk4::Image>,
    #[template_child]
    pub targets: TemplateChild<gtk4::Box>,
    #[template_child]
    pub target_gap: TemplateChild<gtk4::Separator>,
    #[template_child]
    pub screen: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub window: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub crop: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub arrow: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub line: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub freehand: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub highlighter: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub rect: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub filled_rect: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub oval: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub number: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub text: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub blur: TemplateChild<gtk4::ToggleButton>,
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
    pub dot: TemplateChild<gtk4::Box>,
    #[template_child]
    pub strokes: TemplateChild<adw::ToggleGroup>,
    #[template_child]
    pub apply_crop: TemplateChild<gtk4::Button>,
    #[template_child]
    pub history_gap: TemplateChild<gtk4::Separator>,
    #[template_child]
    pub history: TemplateChild<gtk4::Box>,
    #[template_child]
    pub undo: TemplateChild<gtk4::Button>,
    #[template_child]
    pub redo: TemplateChild<gtk4::Button>,
    #[template_child]
    pub finish_gap: TemplateChild<gtk4::Separator>,
    #[template_child]
    pub finish: TemplateChild<gtk4::Box>,
    #[template_child]
    pub copy_close: TemplateChild<adw::SplitButton>,
    #[template_child]
    pub cancel: TemplateChild<gtk4::Button>,

    pub tool: Cell<&'static str>,
    pub target: Cell<&'static str>,
    pub color: Cell<u32>,
    pub stroke: Cell<u32>,
    pub can_undo: Cell<bool>,
    pub can_redo: Cell<bool>,
    pub drag_from: Cell<Option<(f64, f64)>>,
    pub finishing: gtk4::gio::SimpleActionGroup,
}

#[glib::object_subclass]
impl ObjectSubclass for ShotToolbar {
    const NAME: &'static str = "ShotToolbar";
    type Type = super::ShotToolbar;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BinLayout>();
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Toolbar);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for ShotToolbar {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("tool-selected")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("target-selected")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("color-selected")
                    .param_types([u32::static_type()])
                    .build(),
                glib::subclass::Signal::builder("stroke-selected")
                    .param_types([u32::static_type()])
                    .build(),
                glib::subclass::Signal::builder("action")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("moved")
                    .param_types([f64::static_type(), f64::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        self.tool.set("arrow");
        self.target.set("screen");
        self.stroke.set(2);
        self.obj().wire();
    }

    fn dispose(&self) {
        self.dispose_template();
        while let Some(child) = self.obj().first_child() {
            child.unparent();
        }
    }
}

impl WidgetImpl for ShotToolbar {}
