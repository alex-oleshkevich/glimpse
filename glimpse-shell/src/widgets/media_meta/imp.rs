use glib::subclass::Signal;
use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
use std::sync::OnceLock;

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/media_meta.ui")]
pub struct MediaMeta {
    #[template_child]
    pub title: TemplateChild<gtk4::Label>,
    #[template_child]
    pub subtitle: TemplateChild<gtk4::Label>,
}

#[glib::object_subclass]
impl ObjectSubclass for MediaMeta {
    const NAME: &'static str = "MediaMeta";
    type Type = super::MediaMeta;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for MediaMeta {
    fn constructed(&self) {
        self.parent_constructed();

        let click = gtk4::GestureClick::new();
        click.set_button(1);
        let obj = self.obj().downgrade();
        click.connect_pressed(move |_, _, _, _| {
            if let Some(meta) = obj.upgrade() {
                meta.emit_by_name::<()>("activated", &[]);
            }
        });
        self.obj().add_controller(click);
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| vec![Signal::builder("activated").build()])
    }
}

impl WidgetImpl for MediaMeta {}
impl BoxImpl for MediaMeta {}
