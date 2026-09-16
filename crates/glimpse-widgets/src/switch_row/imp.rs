use std::cell::Cell;
use std::marker::PhantomData;
use std::sync::OnceLock;

use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};

use super::TOGGLED;

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::SwitchRow)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/switch_row.ui")]
pub struct SwitchRow {
    #[template_child]
    pub knob: TemplateChild<gtk4::Switch>,

    #[property(name = "active", get = Self::active, set = Self::set_active)]
    active: PhantomData<bool>,

    pub quiet: Cell<bool>,
}

impl SwitchRow {
    fn active(&self) -> bool {
        self.knob.is_active()
    }

    fn set_active(&self, active: bool) {
        if self.knob.is_active() == active {
            return;
        }
        self.quiet.set(true);
        self.knob.set_active(active);
        self.quiet.set(false);
    }
}

#[glib::object_subclass]
impl ObjectSubclass for SwitchRow {
    const NAME: &'static str = "SwitchRow";
    type Type = super::SwitchRow;
    type ParentType = crate::Row;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

#[glib::derived_properties]
impl ObjectImpl for SwitchRow {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder(TOGGLED)
                    .param_types([bool::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let row = self.obj();

        self.knob.connect_active_notify(glib::clone!(
            #[weak(rename_to = row)]
            row,
            move |knob| {
                row.notify_active();
                if row.imp().quiet.get() {
                    return;
                }
                row.emit_by_name::<()>(TOGGLED, &[&knob.is_active()]);
            }
        ));

        row.connect_clicked(|row| {
            let knob = row.imp().knob.get();
            knob.set_active(!knob.is_active());
        });
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for SwitchRow {}
impl ButtonImpl for SwitchRow {}
impl crate::row::RowImpl for SwitchRow {}
