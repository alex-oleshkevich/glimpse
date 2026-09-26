use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{Calendar, EventList, Hero, PopoverShell, Row, Section, WorldClock};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/calendar_popover.ui")]
pub struct CalendarPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub hero: TemplateChild<Hero>,
    #[template_child]
    pub calendar: TemplateChild<Calendar>,
    #[template_child]
    pub day: TemplateChild<Section>,
    #[template_child]
    pub events: TemplateChild<EventList>,
    #[template_child]
    pub truncated: TemplateChild<gtk4::Label>,
    pub day_truncated: std::cell::Cell<bool>,
    #[template_child]
    pub zones: TemplateChild<Section>,
    #[template_child]
    pub clocks: TemplateChild<WorldClock>,
    #[template_child]
    pub footer: TemplateChild<Row>,
}

#[glib::object_subclass]
impl ObjectSubclass for CalendarPopover {
    const NAME: &'static str = "CalendarPopover";
    type Type = super::CalendarPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for CalendarPopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("day-selected")
                    .param_types([i32::static_type(), u32::static_type(), u32::static_type()])
                    .build(),
                glib::subclass::Signal::builder("month-shown")
                    .param_types([i32::static_type(), u32::static_type()])
                    .build(),
                glib::subclass::Signal::builder("footer-activated").build(),
                glib::subclass::Signal::builder("link-activated")
                    .param_types([String::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let popover = self.obj();

        self.events.set_max_rows(super::MAX_ROWS);

        self.calendar.connect_day_selected(glib::clone!(
            #[weak]
            popover,
            move |_, date| {
                popover.emit_by_name::<()>("day-selected", &[&date.year, &date.month, &date.day]);
            }
        ));

        self.calendar.connect_month_shown(glib::clone!(
            #[weak]
            popover,
            move |_, year, month| {
                popover.emit_by_name::<()>("month-shown", &[&year, &month]);
            }
        ));

        self.events.connect_link_activated(glib::clone!(
            #[weak]
            popover,
            move |_, url| popover.emit_by_name::<()>("link-activated", &[&url])
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

impl WidgetImpl for CalendarPopover {}
