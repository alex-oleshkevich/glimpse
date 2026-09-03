use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{Hero, PopoverShell, WorkspaceList};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/workspaces_popover.ui")]
pub struct WorkspacesPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub hero: TemplateChild<Hero>,
    #[template_child]
    pub list: TemplateChild<WorkspaceList>,
    #[template_child]
    pub drawer: TemplateChild<gtk4::Revealer>,
    #[template_child]
    pub detail: TemplateChild<crate::Section>,
    #[template_child]
    pub page: TemplateChild<gtk4::Box>,
    pub opened: std::cell::Cell<Option<u64>>,
    pub rows: std::cell::RefCell<Vec<(u64, crate::Row)>>,
    pub workspaces: std::cell::RefCell<Vec<crate::Workspace>>,
}

#[glib::object_subclass]
impl ObjectSubclass for WorkspacesPopover {
    const NAME: &'static str = "WorkspacesPopover";
    type Type = super::WorkspacesPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for WorkspacesPopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("window-activated")
                    .param_types([u64::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let popover = self.obj();
        self.list.connect_details(glib::clone!(
            #[weak]
            popover,
            move |_, id| popover.toggle_detail(id)
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for WorkspacesPopover {}
