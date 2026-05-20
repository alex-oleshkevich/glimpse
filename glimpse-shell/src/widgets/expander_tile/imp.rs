use glib::subclass::Signal;
use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
use std::{cell::Cell, sync::OnceLock};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/expander_tile.ui")]
pub struct ExpanderTile {
    #[template_child]
    pub header: TemplateChild<gtk4::Box>,
    #[template_child]
    pub left_slot: TemplateChild<gtk4::Box>,
    #[template_child]
    pub primary_label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub secondary_label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub chevron: TemplateChild<gtk4::Image>,
    #[template_child]
    pub revealer: TemplateChild<gtk4::Revealer>,
    #[template_child]
    pub child_slot: TemplateChild<gtk4::Box>,

    pub(super) expanded: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for ExpanderTile {
    const NAME: &'static str = "GlimpseExpanderTile";
    type Type = super::ExpanderTile;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for ExpanderTile {
    fn constructed(&self) {
        self.parent_constructed();

        let obj = self.obj().downgrade();
        let gesture = gtk4::GestureClick::new();
        gesture.connect_released(move |gesture, _, x, y| {
            tracing::debug!(x, y, "ExpanderTile: header released");
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            if let Some(tile) = obj.upgrade() {
                let new_state = !tile.imp().expanded.get();
                tile.apply_expanded(new_state);
                tile.emit_by_name::<()>("expanded", &[&new_state]);
            }
        });
        self.header.add_controller(gesture);
        self.header
            .add_controller(gtk4::EventControllerMotion::new());
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                Signal::builder("expanded")
                    .param_types([bool::static_type()])
                    .build(),
            ]
        })
    }
}

impl WidgetImpl for ExpanderTile {}
impl BoxImpl for ExpanderTile {}
