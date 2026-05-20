use glib::subclass::Signal;
use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
use std::sync::OnceLock;

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/slider_tile.ui")]
pub struct SliderTile {
    #[template_child]
    pub label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub left_slot: TemplateChild<gtk4::Box>,
    #[template_child]
    pub slider: TemplateChild<gtk4::Scale>,
}

#[glib::object_subclass]
impl ObjectSubclass for SliderTile {
    const NAME: &'static str = "GlimpseSliderTile";
    type Type = super::SliderTile;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for SliderTile {
    fn constructed(&self) {
        self.parent_constructed();

        let obj = self.obj().downgrade();
        self.slider.connect_value_changed(move |scale| {
            if let Some(tile) = obj.upgrade() {
                tile.emit_by_name::<()>("changed", &[&scale.value()]);
            }
        });
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                Signal::builder("changed")
                    .param_types([f64::static_type()])
                    .build(),
            ]
        })
    }
}

impl WidgetImpl for SliderTile {}
impl BoxImpl for SliderTile {}
