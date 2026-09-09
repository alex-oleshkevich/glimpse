use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{NowPlaying, Placeholder, PlayerList, PopoverShell, Row, Section};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/mpris_popover.ui")]
pub struct MprisPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub player: TemplateChild<NowPlaying>,
    #[template_child]
    pub others: TemplateChild<Section>,
    #[template_child]
    pub list: TemplateChild<PlayerList>,
    #[template_child]
    pub nothing: TemplateChild<Placeholder>,
    #[template_child]
    pub footer: TemplateChild<Row>,
}

#[glib::object_subclass]
impl ObjectSubclass for MprisPopover {
    const NAME: &'static str = "MprisPopover";
    type Type = super::MprisPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for MprisPopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("footer-activated").build(),
                glib::subclass::Signal::builder("raise-requested")
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder("toggle-requested")
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

        self.list.connect_activated(glib::clone!(
            #[weak]
            popover,
            move |_, key| popover.emit_by_name::<()>("raise-requested", &[&key])
        ));

        self.list.connect_toggled(glib::clone!(
            #[weak]
            popover,
            move |_, key| popover.emit_by_name::<()>("toggle-requested", &[&key])
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for MprisPopover {}
