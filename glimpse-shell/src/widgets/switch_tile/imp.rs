use glib::subclass::Signal;
use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
use std::sync::OnceLock;

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/switch_tile.ui")]
pub struct SwitchTile {
    #[template_child] pub left_slot:       TemplateChild<gtk4::Box>,
    #[template_child] pub primary_label:   TemplateChild<gtk4::Label>,
    #[template_child] pub secondary_label: TemplateChild<gtk4::Label>,
    #[template_child] pub toggle:          TemplateChild<gtk4::Switch>,
}

#[glib::object_subclass]
impl ObjectSubclass for SwitchTile {
    const NAME: &'static str = "GlimpseSwitchTile";
    type Type = super::SwitchTile;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for SwitchTile {
    fn constructed(&self) {
        self.parent_constructed();

        let obj = self.obj().downgrade();
        self.toggle.connect_state_set(move |_, state| {
            if let Some(tile) = obj.upgrade() {
                tile.emit_by_name::<()>("toggled", &[&state]);
            }
            glib::Propagation::Proceed
        });
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![Signal::builder("toggled")
                .param_types([bool::static_type()])
                .build()]
        })
    }
}

impl WidgetImpl for SwitchTile {}
impl BoxImpl for SwitchTile {}
