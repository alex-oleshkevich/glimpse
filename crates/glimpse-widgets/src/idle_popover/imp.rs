use std::cell::Cell;
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{ActionRow, ExpanderRow, InhibitorList, PopoverShell, Section, ToggleRow};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/idle_popover.ui")]
pub struct IdlePopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub hold: TemplateChild<gtk4::Switch>,
    #[template_child]
    pub hold_card: TemplateChild<ExpanderRow>,
    #[template_child]
    pub preset_15m: TemplateChild<ToggleRow>,
    #[template_child]
    pub preset_30m: TemplateChild<ToggleRow>,
    #[template_child]
    pub preset_1h: TemplateChild<ToggleRow>,
    #[template_child]
    pub preset_2h: TemplateChild<ToggleRow>,
    #[template_child]
    pub preset_4h: TemplateChild<ToggleRow>,
    #[template_child]
    pub others: TemplateChild<Section>,
    #[template_child]
    pub list: TemplateChild<InhibitorList>,

    pub quiet: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for IdlePopover {
    const NAME: &'static str = "IdlePopover";
    type Type = super::IdlePopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        ActionRow::static_type();
        ExpanderRow::static_type();
        ToggleRow::static_type();
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for IdlePopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("hold-toggled")
                    .param_types([bool::static_type()])
                    .build(),
                glib::subclass::Signal::builder("hold-requested")
                    .param_types([u32::static_type()])
                    .build(),
                glib::subclass::Signal::builder("release-requested")
                    .param_types([u64::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let popover = self.obj();

        self.hold.connect_active_notify(glib::clone!(
            #[weak]
            popover,
            move |switch| {
                if popover.imp().quiet.get() {
                    return;
                }
                popover.emit_by_name::<()>("hold-toggled", &[&switch.is_active()]);
            }
        ));

        for (row, seconds) in self.presets() {
            row.connect_toggled(glib::clone!(
                #[weak]
                popover,
                move |_, _| {
                    popover.set_hold_preset(Some(seconds));
                    popover.emit_by_name::<()>("hold-requested", &[&seconds]);
                }
            ));
        }

        self.list.connect_release_requested(glib::clone!(
            #[weak]
            popover,
            move |_, id| popover.emit_by_name::<()>("release-requested", &[&id])
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for IdlePopover {}

impl IdlePopover {
    pub fn presets(&self) -> [(&ToggleRow, u32); 5] {
        [
            (&self.preset_15m, 900),
            (&self.preset_30m, 1800),
            (&self.preset_1h, 3600),
            (&self.preset_2h, 7200),
            (&self.preset_4h, 14400),
        ]
    }
}
