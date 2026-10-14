use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{ActionRow, ExpanderRow, PopoverShell, Section};

use super::BluetoothDevice;

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
    pub problem: String,
    pub icon: String,
    pub place: Place,
    pub busy: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Ask {
    pub key: String,
    pub device: String,
    pub question: String,
    pub code: String,
    pub progress: String,
    pub accept: String,
    pub cancel: String,
    pub destructive: bool,
}

pub use crate::fact_list::{Details, Line};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/bluetooth_popover.ui")]
pub struct BluetoothPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub power: TemplateChild<gtk4::Switch>,
    #[template_child]
    pub pages: TemplateChild<gtk4::Stack>,
    #[template_child]
    pub prompt_device: TemplateChild<gtk4::Label>,
    #[template_child]
    pub prompt_ask: TemplateChild<gtk4::Label>,
    #[template_child]
    pub prompt_code: TemplateChild<gtk4::Label>,
    #[template_child]
    pub prompt_progress: TemplateChild<gtk4::Label>,
    #[template_child]
    pub prompt_actions: TemplateChild<gtk4::Box>,
    #[template_child]
    pub prompt_cancel: TemplateChild<gtk4::Button>,
    #[template_child]
    pub prompt_accept: TemplateChild<gtk4::Button>,
    #[template_child]
    pub devices: TemplateChild<Section>,
    #[template_child]
    pub device_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub more_paired: TemplateChild<ActionRow>,
    #[template_child]
    pub more_nearby: TemplateChild<ActionRow>,
    #[template_child]
    pub nearby: TemplateChild<Section>,
    #[template_child]
    pub nearby_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub search: TemplateChild<ExpanderRow>,

    pub entries: RefCell<Vec<Entry>>,
    pub details: RefCell<Vec<Details>>,
    pub prompt: RefCell<Option<Ask>>,
    pub devices_held: RefCell<Vec<(String, BluetoothDevice)>>,
    pub nearby_held: RefCell<Vec<(String, BluetoothDevice)>>,
    pub quiet: std::cell::Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for BluetoothPopover {
    const NAME: &'static str = "BluetoothPopover";
    type Type = super::BluetoothPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        ActionRow::static_type();
        ExpanderRow::static_type();
        BluetoothDevice::static_type();
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
                glib::subclass::Signal::builder("nearby-toggled").build(),
                glib::subclass::Signal::builder("expanded")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("answered")
                    .param_types([bool::static_type()])
                    .build(),
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
        self.search.connect_toggled(glib::clone!(
            #[weak]
            popover,
            move |_, _| {
                if !popover.imp().quiet.get() {
                    popover.emit_by_name::<()>("nearby-toggled", &[]);
                }
            }
        ));
        self.more_paired.connect_activated(glib::clone!(
            #[weak]
            popover,
            move |_| popover.emit_by_name::<()>("expanded", &[&"paired".to_owned()])
        ));
        self.more_nearby.connect_activated(glib::clone!(
            #[weak]
            popover,
            move |_| popover.emit_by_name::<()>("expanded", &[&"nearby".to_owned()])
        ));

        self.prompt_cancel.connect_clicked(glib::clone!(
            #[weak]
            popover,
            move |_| popover.emit_by_name::<()>("answered", &[&false])
        ));
        self.prompt_accept.connect_clicked(glib::clone!(
            #[weak]
            popover,
            move |_| popover.emit_by_name::<()>("answered", &[&true])
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for BluetoothPopover {}
