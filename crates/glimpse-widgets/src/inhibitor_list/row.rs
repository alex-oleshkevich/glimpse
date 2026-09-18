use gettextrs::gettext;
use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use super::InhibitorTargets;

const CHIP_CLASS: &str = "inhibitor-row__chip";
const CHIP_MINOR_CLASS: &str = "inhibitor-row__chip--minor";
const NAME_JOIN: &str = ". ";

fn chip_labels(targets: &InhibitorTargets) -> Vec<(String, bool)> {
    [
        (gettext("Idle"), targets.idle, false),
        (gettext("Suspend"), targets.suspend, false),
        (gettext("Shutdown"), targets.shutdown, false),
        (gettext("Lid"), targets.lid_switch, false),
        (gettext("Power key"), targets.power_key, true),
        (gettext("Suspend key"), targets.suspend_key, true),
        (gettext("Hibernate key"), targets.hibernate_key, true),
    ]
    .into_iter()
    .filter(|(_, on, _)| *on)
    .map(|(label, _, minor)| (label, minor))
    .collect()
}

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/me/aresa/GlimpseShell/widgets/inhibitor_row.ui")]
    pub struct InhibitorRow {
        #[template_child]
        pub chips: TemplateChild<gtk4::Box>,
        #[template_child]
        pub release: TemplateChild<gtk4::Button>,
        pub id: Cell<u64>,
        pub targets: Cell<InhibitorTargets>,
        pub accessible_name: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for InhibitorRow {
        const NAME: &'static str = "InhibitorRow";
        type Type = super::InhibitorRow;
        type ParentType = crate::Row;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for InhibitorRow {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    glib::subclass::Signal::builder("release-requested")
                        .param_types([u64::static_type()])
                        .build(),
                ]
            })
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj().clone();
            self.release.connect_clicked(glib::clone!(
                #[weak]
                obj,
                move |_| {
                    let id = obj.imp().id.get();
                    obj.emit_by_name::<()>("release-requested", &[&id]);
                }
            ));
        }

        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for InhibitorRow {}
    impl ButtonImpl for InhibitorRow {}
    impl crate::row::RowImpl for InhibitorRow {}
}

glib::wrapper! {
    pub struct InhibitorRow(ObjectSubclass<imp::InhibitorRow>)
        @extends crate::Row, gtk4::Button, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Actionable, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for InhibitorRow {
    fn default() -> Self {
        Self::new()
    }
}

impl InhibitorRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_id(&self, id: u64) {
        self.imp().id.set(id);
    }

    pub fn set_can_release(&self, can_release: bool) {
        let release = &self.imp().release;
        if release.get_visible() == can_release {
            return;
        }
        release.set_visible(can_release);
    }

    pub fn set_targets(&self, targets: &InhibitorTargets) {
        let imp = self.imp();
        if imp.targets.get() == *targets {
            return;
        }
        imp.targets.set(*targets);
        crate::clear_children(&imp.chips);
        for (label, minor) in chip_labels(targets) {
            let chip = gtk4::Label::new(Some(&label));
            chip.add_css_class(CHIP_CLASS);
            if minor {
                chip.add_css_class(CHIP_MINOR_CLASS);
            }
            imp.chips.append(&chip);
        }
    }

    pub fn sync_accessible_label(&self) {
        let row: &crate::Row = self.upcast_ref();
        let title = row.title().unwrap_or_default();
        let chips = chip_labels(&self.imp().targets.get());

        let name = std::iter::once(title)
            .chain(chips.into_iter().map(|(label, _)| label))
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(NAME_JOIN);

        let imp = self.imp();
        if *imp.accessible_name.borrow() == name {
            return;
        }
        imp.accessible_name.replace(name.clone());
        if name.is_empty() {
            self.reset_property(gtk4::AccessibleProperty::Label);
        } else {
            self.update_property(&[gtk4::accessible::Property::Label(&name)]);
        }
    }

    pub fn connect_release_requested<F: Fn(&Self, u64) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "release-requested",
            false,
            glib::closure_local!(move |row: Self, id: u64| f(&row, id)),
        )
    }
}
