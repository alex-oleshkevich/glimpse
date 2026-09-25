use std::cell::Cell;

use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};

#[derive(Debug, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/shot_countdown.ui")]
pub struct ShotCountdown {
    #[template_child]
    pub label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub cancel: TemplateChild<gtk4::Button>,

    pub seconds: Cell<u32>,
}

impl Default for ShotCountdown {
    fn default() -> Self {
        Self {
            label: TemplateChild::default(),
            cancel: TemplateChild::default(),
            seconds: Cell::new(u32::MAX),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for ShotCountdown {
    const NAME: &'static str = "ShotCountdown";
    type Type = super::ShotCountdown;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BinLayout>();
        klass.bind_template();
        klass.set_accessible_role(gtk4::AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for ShotCountdown {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: std::sync::OnceLock<Vec<glib::subclass::Signal>> =
            std::sync::OnceLock::new();
        SIGNALS.get_or_init(|| vec![glib::subclass::Signal::builder("cancelled").build()])
    }

    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.set_seconds(0);
        self.cancel.connect_clicked(glib::clone!(
            #[weak]
            obj,
            move |_| obj.emit_by_name::<()>("cancelled", &[])
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
        while let Some(child) = self.obj().first_child() {
            child.unparent();
        }
    }
}

impl WidgetImpl for ShotCountdown {}
