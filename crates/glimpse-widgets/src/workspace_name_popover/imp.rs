use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, gdk, glib, prelude::*, subclass::prelude::*,
};

use crate::{Hero, PopoverShell, Row};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/workspace_name_popover.ui")]
pub struct WorkspaceNamePopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub hero: TemplateChild<Hero>,
    #[template_child]
    pub name: TemplateChild<gtk4::Entry>,
    #[template_child]
    pub footer: TemplateChild<Row>,
    pub given: RefCell<String>,
}

#[glib::object_subclass]
impl ObjectSubclass for WorkspaceNamePopover {
    const NAME: &'static str = "WorkspaceNamePopover";
    type Type = super::WorkspaceNamePopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for WorkspaceNamePopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("submitted")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("cancelled").build(),
                glib::subclass::Signal::builder("footer-activated").build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let popover = self.obj();

        self.name.connect_activate(glib::clone!(
            #[weak]
            popover,
            move |entry| {
                popover.emit_by_name::<()>("submitted", &[&entry.text().to_string()]);
            }
        ));

        self.name.connect_icon_press(|entry, position| {
            if position == gtk4::EntryIconPosition::Secondary {
                entry.set_text("");
                entry.grab_focus();
            }
        });

        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        keys.connect_key_pressed(glib::clone!(
            #[weak]
            popover,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, _| {
                if key != gdk::Key::Escape {
                    return glib::Propagation::Proceed;
                }
                popover.emit_by_name::<()>("cancelled", &[]);
                glib::Propagation::Stop
            }
        ));
        popover.add_controller(keys);
        popover.connect_map(|popover| popover.focus_entry());

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

impl WidgetImpl for WorkspaceNamePopover {}
