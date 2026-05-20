use glib::subclass::Signal;
use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
use std::{cell::Cell, sync::OnceLock};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/choice_tile.ui")]
pub struct ChoiceTile {
    #[template_child]
    pub left_slot: TemplateChild<gtk4::Box>,
    #[template_child]
    pub primary_label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub secondary_label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub checkmark: TemplateChild<gtk4::Image>,

    pub(super) selected: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for ChoiceTile {
    const NAME: &'static str = "GlimpseChoiceTile";
    type Type = super::ChoiceTile;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for ChoiceTile {
    fn constructed(&self) {
        self.parent_constructed();

        let obj = self.obj().downgrade();
        let gesture = gtk4::GestureClick::new();
        gesture.connect_released(move |_, _, _, _| {
            if let Some(tile) = obj.upgrade() {
                tile.emit_by_name::<()>("activated", &[]);
            }
        });
        self.obj().add_controller(gesture);
        self.obj()
            .add_controller(gtk4::EventControllerMotion::new());
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| vec![Signal::builder("activated").build()])
    }
}

impl WidgetImpl for ChoiceTile {}
impl BoxImpl for ChoiceTile {}
