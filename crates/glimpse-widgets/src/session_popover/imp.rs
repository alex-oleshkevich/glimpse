use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{Hero, PopoverShell, Row, Section};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/session_popover.ui")]
pub struct SessionPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub hero: TemplateChild<Hero>,
    #[template_child]
    pub lock: TemplateChild<Row>,
    #[template_child]
    pub suspend: TemplateChild<Row>,
    #[template_child]
    pub hibernate: TemplateChild<Row>,
    #[template_child]
    pub logout: TemplateChild<Row>,
    #[template_child]
    pub reboot: TemplateChild<Row>,
    #[template_child]
    pub power_off: TemplateChild<Row>,
    #[template_child]
    pub footer: TemplateChild<Row>,
}

#[glib::object_subclass]
impl ObjectSubclass for SessionPopover {
    const NAME: &'static str = "SessionPopover";
    type Type = super::SessionPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        PopoverShell::static_type();
        Hero::static_type();
        Section::static_type();
        Row::static_type();
        klass.set_layout_manager_type::<gtk4::BinLayout>();
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for SessionPopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("action-requested")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("footer-activated").build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let popover = self.obj();
        for (row, action) in [
            (&self.lock, super::LOCK),
            (&self.suspend, super::SUSPEND),
            (&self.hibernate, super::HIBERNATE),
            (&self.logout, super::LOG_OUT),
            (&self.reboot, super::REBOOT),
            (&self.power_off, super::POWER_OFF),
        ] {
            row.connect_clicked(glib::clone!(
                #[weak]
                popover,
                move |_| popover.emit_by_name::<()>("action-requested", &[&action])
            ));
        }
        self.footer.connect_clicked(glib::clone!(
            #[weak]
            popover,
            move |_| popover.emit_by_name::<()>("footer-activated", &[])
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for SessionPopover {}
