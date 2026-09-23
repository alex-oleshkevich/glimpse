use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
use std::sync::OnceLock;

use crate::Indicator;

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/status_island.ui")]
pub struct StatusIsland {
    #[template_child]
    pub weather: TemplateChild<Indicator>,
    #[template_child]
    pub bluetooth: TemplateChild<Indicator>,
    #[template_child]
    pub network: TemplateChild<Indicator>,
    #[template_child]
    pub layout: TemplateChild<Indicator>,
    #[template_child]
    pub battery: TemplateChild<Indicator>,
    #[template_child]
    pub power_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub power_indicator: TemplateChild<Indicator>,
}

#[glib::object_subclass]
impl ObjectSubclass for StatusIsland {
    const NAME: &'static str = "StatusIsland";
    type Type = super::StatusIsland;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        Indicator::static_type();
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for StatusIsland {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| vec![glib::subclass::Signal::builder("session-toggled").build()])
    }

    fn constructed(&self) {
        self.parent_constructed();

        if let Ok(icon) = gio::Icon::for_string("system-shutdown-symbolic") {
            self.power_indicator.set_icon(Some(&icon));
        }

        let island = self.obj();
        self.power_button.connect_clicked(glib::clone!(
            #[weak]
            island,
            move |_| island.emit_by_name::<()>("session-toggled", &[])
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for StatusIsland {}
