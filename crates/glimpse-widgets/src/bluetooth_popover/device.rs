use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Row, RowTone, none_if_empty};

use super::{Details, Entry};

mod imp {
    use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
    use std::cell::RefCell;

    use crate::{ActionRow, ExpanderRow, FactList, ToggleRow};

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/me/aresa/GlimpseShell/widgets/bluetooth_device.ui")]
    pub struct BluetoothDevice {
        #[template_child]
        pub row: TemplateChild<ExpanderRow>,
        #[template_child]
        pub status: TemplateChild<gtk4::Image>,
        #[template_child]
        pub primary: TemplateChild<ActionRow>,
        #[template_child]
        pub trust: TemplateChild<ToggleRow>,
        #[template_child]
        pub forget: TemplateChild<ActionRow>,
        #[template_child]
        pub facts: TemplateChild<FactList>,

        pub primary_action: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for BluetoothDevice {
        const NAME: &'static str = "BluetoothDevice";
        type Type = super::BluetoothDevice;
        type ParentType = gtk4::Widget;

        fn class_init(klass: &mut Self::Class) {
            ActionRow::static_type();
            ExpanderRow::static_type();
            ToggleRow::static_type();
            FactList::static_type();
            klass.bind_template();
            klass.set_accessible_role(gtk4::AccessibleRole::Group);
        }

        fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
            object.init_template();
        }
    }

    impl ObjectImpl for BluetoothDevice {
        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for BluetoothDevice {}
}

glib::wrapper! {
    pub struct BluetoothDevice(ObjectSubclass<imp::BluetoothDevice>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl BluetoothDevice {
    pub fn set_entry(&self, entry: &Entry) {
        let row = &self.imp().row;
        row.set_title(none_if_empty(&entry.title));
        row.set_subtitle(none_if_empty(&entry.problem));
        row.set_tone(if entry.problem.is_empty() {
            RowTone::Normal
        } else {
            RowTone::Warning
        });
        row.set_icon_name(none_if_empty(&entry.icon));
        row.set_busy(entry.busy);
        self.imp()
            .status
            .set_visible(entry.place == super::Place::Connected);
    }

    pub fn set_details(&self, details: Option<&Details>) {
        let imp = self.imp();
        if imp.row.get_sensitive() != details.is_some() {
            imp.row.set_sensitive(details.is_some());
        }
        if details.is_none() {
            imp.row.set_open(false);
        }
        let lines = details.map_or(&[][..], |details| details.lines.as_slice());
        let primary = lines
            .iter()
            .find(|line| matches!(line.action.as_str(), "connect" | "disconnect" | "pair"));
        imp.primary_action
            .replace(primary.map_or_else(String::new, |line| line.action.clone()));
        for (row, line) in [
            (imp.primary.upcast_ref::<Row>(), primary),
            (
                imp.forget.upcast_ref::<Row>(),
                lines.iter().find(|line| line.action == "forget"),
            ),
        ] {
            if row.get_visible() != line.is_some() {
                row.set_visible(line.is_some());
            }
            if let Some(line) = line {
                row.set_title(none_if_empty(&line.title));
                row.set_busy(line.busy);
                row.set_inactive(!line.activates);
                row.set_tone(if line.destructive {
                    RowTone::Destructive
                } else {
                    RowTone::Normal
                });
            }
        }
        let trust = lines.iter().find(|line| line.action == "trust");
        if imp.trust.get_visible() != trust.is_some() {
            imp.trust.set_visible(trust.is_some());
        }
        if let Some(line) = trust {
            let row = imp.trust.upcast_ref::<Row>();
            row.set_title(none_if_empty(&line.title));
            row.set_busy(line.busy);
            imp.trust.set_active(line.toggle.unwrap_or_default());
        }
        let facts = details.map_or(&[][..], |details| details.facts.as_slice());
        imp.facts.set_facts(facts);
        if imp.facts.get_visible() == facts.is_empty() {
            imp.facts.set_visible(!facts.is_empty());
        }
    }
}
