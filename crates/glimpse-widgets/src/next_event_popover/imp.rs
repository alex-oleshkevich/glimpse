use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{EventList, Hero, Placeholder, PopoverShell, Readout, Row, Section};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/next_event_popover.ui")]
pub struct NextEventPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub hero: TemplateChild<Hero>,
    #[template_child]
    pub countdown: TemplateChild<Readout>,
    #[template_child]
    pub upcoming: TemplateChild<Section>,
    #[template_child]
    pub later: TemplateChild<EventList>,
    #[template_child]
    pub nothing: TemplateChild<Placeholder>,
    #[template_child]
    pub footer: TemplateChild<Row>,
    pub quiet: RefCell<(String, String)>,
}

#[glib::object_subclass]
impl ObjectSubclass for NextEventPopover {
    const NAME: &'static str = "NextEventPopover";
    type Type = super::NextEventPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for NextEventPopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| vec![glib::subclass::Signal::builder("footer-activated").build()])
    }

    fn constructed(&self) {
        self.parent_constructed();
        let popover = self.obj();

        self.quiet.replace((
            self.hero.title().unwrap_or_default(),
            self.hero.subtitle().unwrap_or_default(),
        ));

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

impl WidgetImpl for NextEventPopover {}
