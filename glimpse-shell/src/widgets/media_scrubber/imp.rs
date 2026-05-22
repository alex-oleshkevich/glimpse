use glib::subclass::Signal;
use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
use std::cell::Cell;
use std::sync::OnceLock;

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/media_scrubber.ui")]
pub struct MediaScrubber {
    #[template_child]
    pub scale: TemplateChild<gtk4::Scale>,
    pub updating: Cell<bool>,
    pub seekable: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for MediaScrubber {
    const NAME: &'static str = "MediaScrubber";
    type Type = super::MediaScrubber;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for MediaScrubber {
    fn constructed(&self) {
        self.parent_constructed();

        let obj = self.obj().downgrade();
        self.scale.connect_value_changed(move |scale| {
            let Some(scrubber) = obj.upgrade() else {
                return;
            };
            if scrubber.imp().updating.get() {
                return;
            }
            if !scrubber.imp().seekable.get() {
                return;
            }
            scrubber.emit_by_name::<()>("seek-requested", &[&scale.value()]);
        });
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                Signal::builder("seek-requested")
                    .param_types([f64::static_type()])
                    .build(),
            ]
        })
    }
}

impl WidgetImpl for MediaScrubber {}
impl BoxImpl for MediaScrubber {}
