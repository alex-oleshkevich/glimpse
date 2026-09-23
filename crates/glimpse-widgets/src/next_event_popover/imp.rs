use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{EventList, FactList, Hero, PopoverShell, Readout, Row, Section};

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
    pub join: TemplateChild<Row>,
    #[template_child]
    pub open_event: TemplateChild<Row>,
    #[template_child]
    pub details: TemplateChild<Section>,
    #[template_child]
    pub facts: TemplateChild<FactList>,
    #[template_child]
    pub upcoming: TemplateChild<Section>,
    #[template_child]
    pub later: TemplateChild<EventList>,
    #[template_child]
    pub footer: TemplateChild<Row>,
    pub join_url: RefCell<Option<String>>,
    pub open_event_url: RefCell<Option<String>>,
}

#[glib::object_subclass]
impl ObjectSubclass for NextEventPopover {
    const NAME: &'static str = "NextEventPopover";
    type Type = super::NextEventPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        FactList::static_type();
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
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("footer-activated").build(),
                glib::subclass::Signal::builder("join-activated")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("open-event-activated")
                    .param_types([String::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let popover = self.obj();

        self.footer.connect_clicked(glib::clone!(
            #[weak]
            popover,
            move |_| popover.emit_by_name::<()>("footer-activated", &[])
        ));
        self.join.connect_clicked(glib::clone!(
            #[weak]
            popover,
            move |_| {
                let Some(url) = popover.imp().join_url.borrow().clone() else {
                    return;
                };
                popover.emit_by_name::<()>("join-activated", &[&url]);
            }
        ));
        self.open_event.connect_clicked(glib::clone!(
            #[weak]
            popover,
            move |_| {
                let Some(url) = popover.imp().open_event_url.borrow().clone() else {
                    return;
                };
                popover.emit_by_name::<()>("open-event-activated", &[&url]);
            }
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for NextEventPopover {}
