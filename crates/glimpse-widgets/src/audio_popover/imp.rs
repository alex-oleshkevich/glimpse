use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{Fader, Hero, Placeholder, PopoverShell, Readout, Row, Section};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
    pub value: Option<String>,
    pub selected: bool,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Block {
    pub dir: String,
    pub heading: Option<String>,
    pub volume: f64,
    pub muted: bool,
    pub icon: Option<String>,
    pub adjustable: bool,
    pub devices: Vec<Entry>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Details {
    pub id: String,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/audio_popover.ui")]
pub struct AudioPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub hero: TemplateChild<Hero>,
    #[template_child]
    pub readout: TemplateChild<Readout>,
    #[template_child]
    pub output: TemplateChild<Fader>,
    #[template_child]
    pub outputs: TemplateChild<Section>,
    #[template_child]
    pub output_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub more_outputs: TemplateChild<Row>,
    #[template_child]
    pub input: TemplateChild<Fader>,
    #[template_child]
    pub inputs: TemplateChild<Section>,
    #[template_child]
    pub input_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub more_inputs: TemplateChild<Row>,
    #[template_child]
    pub apps: TemplateChild<Section>,
    #[template_child]
    pub app_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub more_apps: TemplateChild<Row>,
    #[template_child]
    pub quiet: TemplateChild<Placeholder>,
    #[template_child]
    pub footer: TemplateChild<Row>,

    pub outputs_list: RefCell<Vec<Entry>>,
    pub inputs_list: RefCell<Vec<Entry>>,
    pub apps_list: RefCell<Vec<Entry>>,
    pub details: RefCell<Option<Details>>,
    pub output_held: RefCell<Vec<(String, Row)>>,
    pub input_held: RefCell<Vec<(String, Row)>>,
    pub app_held: RefCell<Vec<(String, gtk4::Box)>>,
    pub blocks: RefCell<Vec<(String, gtk4::Box)>>,
    pub block_devices: RefCell<Vec<(String, Row)>>,
}

#[glib::object_subclass]
impl ObjectSubclass for AudioPopover {
    const NAME: &'static str = "AudioPopover";
    type Type = super::AudioPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for AudioPopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("level-changed")
                    .param_types([String::static_type(), f64::static_type()])
                    .build(),
                glib::subclass::Signal::builder("level-moved")
                    .param_types([String::static_type(), f64::static_type()])
                    .build(),
                glib::subclass::Signal::builder("level-toggled")
                    .param_types([String::static_type(), bool::static_type()])
                    .build(),
                glib::subclass::Signal::builder("device-selected")
                    .param_types([String::static_type(), String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("app-selected")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("app-level-changed")
                    .param_types([
                        String::static_type(),
                        String::static_type(),
                        f64::static_type(),
                    ])
                    .build(),
                glib::subclass::Signal::builder("app-level-toggled")
                    .param_types([
                        String::static_type(),
                        String::static_type(),
                        bool::static_type(),
                    ])
                    .build(),
                glib::subclass::Signal::builder("app-moved")
                    .param_types([
                        String::static_type(),
                        String::static_type(),
                        String::static_type(),
                    ])
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

        self.output.connect_changed(glib::clone!(
            #[weak]
            popover,
            move |_, value| {
                popover.emit_by_name::<()>("level-changed", &[&"output".to_owned(), &value])
            }
        ));
        self.output.connect_moved(glib::clone!(
            #[weak]
            popover,
            move |_, value| {
                popover.emit_by_name::<()>("level-moved", &[&"output".to_owned(), &value])
            }
        ));
        self.output.connect_toggled(glib::clone!(
            #[weak]
            popover,
            move |_, muted| {
                popover.emit_by_name::<()>("level-toggled", &[&"output".to_owned(), &muted])
            }
        ));
        self.input.connect_changed(glib::clone!(
            #[weak]
            popover,
            move |_, value| {
                popover.emit_by_name::<()>("level-changed", &[&"input".to_owned(), &value])
            }
        ));
        self.input.connect_toggled(glib::clone!(
            #[weak]
            popover,
            move |_, muted| {
                popover.emit_by_name::<()>("level-toggled", &[&"input".to_owned(), &muted])
            }
        ));

        self.more_outputs.connect_clicked(glib::clone!(
            #[weak]
            popover,
            move |_| popover.emit_by_name::<()>("expanded", &[&"outputs".to_owned()])
        ));
        self.more_inputs.connect_clicked(glib::clone!(
            #[weak]
            popover,
            move |_| popover.emit_by_name::<()>("expanded", &[&"inputs".to_owned()])
        ));
        self.more_apps.connect_clicked(glib::clone!(
            #[weak]
            popover,
            move |_| popover.emit_by_name::<()>("expanded", &[&"apps".to_owned()])
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

impl WidgetImpl for AudioPopover {}
