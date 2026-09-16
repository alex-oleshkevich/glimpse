use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{Hero, PopoverShell, Row, Section};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    #[default]
    Connected,
    Paired,
    Nearby,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub icon: String,
    pub place: Place,
    pub value: String,
    pub selected: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Line {
    pub action: String,
    pub title: String,
    pub value: String,
    pub icon: String,
    pub toggle: Option<bool>,
    pub destructive: bool,
    pub activates: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Details {
    pub id: String,
    pub notice: String,
    pub lines: Vec<Line>,
}

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/bluetooth_popover.ui")]
pub struct BluetoothPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub hero: TemplateChild<Hero>,
    #[template_child]
    pub power: TemplateChild<gtk4::Switch>,
    #[template_child]
    pub connected: TemplateChild<Section>,
    #[template_child]
    pub connected_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub paired: TemplateChild<Section>,
    #[template_child]
    pub paired_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub more_paired: TemplateChild<Row>,
    #[template_child]
    pub more_nearby: TemplateChild<Row>,
    #[template_child]
    pub nearby: TemplateChild<Section>,
    #[template_child]
    pub nearby_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub scan: TemplateChild<Row>,
    #[template_child]
    pub visible_as: TemplateChild<gtk4::Label>,
    #[template_child]
    pub footer: TemplateChild<Row>,

    pub entries: RefCell<Vec<Entry>>,
    pub details: RefCell<Option<Details>>,
    pub connected_held: RefCell<Vec<(String, gtk4::Box)>>,
    pub paired_held: RefCell<Vec<(String, gtk4::Box)>>,
    pub nearby_held: RefCell<Vec<(String, gtk4::Box)>>,
    pub lines: RefCell<Vec<(String, Row)>>,
    pub scanning: std::cell::Cell<bool>,
    pub quiet: std::cell::Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for BluetoothPopover {
    const NAME: &'static str = "BluetoothPopover";
    type Type = super::BluetoothPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for BluetoothPopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("powered")
                    .param_types([bool::static_type()])
                    .build(),
                glib::subclass::Signal::builder("activated")
                    .param_types([String::static_type(), bool::static_type()])
                    .build(),
                glib::subclass::Signal::builder("selected")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("acted")
                    .param_types([String::static_type(), String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("toggled")
                    .param_types([
                        String::static_type(),
                        String::static_type(),
                        bool::static_type(),
                    ])
                    .build(),
                glib::subclass::Signal::builder("scanning")
                    .param_types([bool::static_type()])
                    .build(),
                glib::subclass::Signal::builder("expanded")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("footer-activated").build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let popover = self.obj();

        self.power.connect_active_notify(glib::clone!(
            #[weak]
            popover,
            move |switch| {
                if popover.imp().quiet.get() {
                    return;
                }
                popover.emit_by_name::<()>("powered", &[&switch.is_active()]);
            }
        ));
        self.scan.connect_clicked(glib::clone!(
            #[weak]
            popover,
            move |_| {
                let wanted = !popover.imp().scanning.get();
                popover.emit_by_name::<()>("scanning", &[&wanted]);
            }
        ));
        self.more_paired.connect_clicked(glib::clone!(
            #[weak]
            popover,
            move |_| popover.emit_by_name::<()>("expanded", &[&"paired".to_owned()])
        ));
        self.more_nearby.connect_clicked(glib::clone!(
            #[weak]
            popover,
            move |_| popover.emit_by_name::<()>("expanded", &[&"nearby".to_owned()])
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

impl WidgetImpl for BluetoothPopover {}
