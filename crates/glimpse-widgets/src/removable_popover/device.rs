use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Fact, Row, RowTone, none_if_empty};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Release {
    Eject(String),
    Unmount(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Primary {
    Open,
    Mount,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Entry {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub busy: bool,
    pub inactive: bool,
    pub problem: String,
    pub space: Option<(f64, String)>,
    pub facts: Vec<Fact>,
    pub primary: Option<Primary>,
    pub release: Option<Release>,
}

mod imp {
    use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
    use std::cell::RefCell;

    use crate::{ActionRow, ExpanderRow, FactList, MeterRow};

    use super::Release;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/me/aresa/GlimpseShell/widgets/removable_device.ui")]
    pub struct RemovableDevice {
        #[template_child]
        pub row: TemplateChild<ExpanderRow>,
        #[template_child]
        pub space: TemplateChild<MeterRow>,
        #[template_child]
        pub facts: TemplateChild<FactList>,
        #[template_child]
        pub primary: TemplateChild<ActionRow>,
        #[template_child]
        pub release: TemplateChild<ActionRow>,

        pub(crate) release_action: RefCell<Option<Release>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RemovableDevice {
        const NAME: &'static str = "RemovableDevice";
        type Type = super::RemovableDevice;
        type ParentType = gtk4::Widget;

        fn class_init(klass: &mut Self::Class) {
            ActionRow::static_type();
            ExpanderRow::static_type();
            MeterRow::static_type();
            FactList::static_type();
            klass.bind_template();
            klass.set_accessible_role(gtk4::AccessibleRole::Group);
        }

        fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
            object.init_template();
        }
    }

    impl ObjectImpl for RemovableDevice {
        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for RemovableDevice {}
}

glib::wrapper! {
    pub struct RemovableDevice(ObjectSubclass<imp::RemovableDevice>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

fn show(row: &Row, visible: bool) {
    if row.get_visible() != visible {
        row.set_visible(visible);
    }
}

impl RemovableDevice {
    pub(super) fn set_entry(&self, entry: &Entry) {
        let imp = self.imp();
        let row = &imp.row;
        row.set_title(none_if_empty(&entry.title));
        row.set_icon_name(none_if_empty(&entry.icon));
        row.set_busy(entry.busy);
        row.set_inactive(entry.inactive);
        row.set_subtitle(none_if_empty(&entry.problem));
        row.set_tone(if entry.problem.is_empty() {
            RowTone::Normal
        } else {
            RowTone::Warning
        });

        let space: &Row = imp.space.upcast_ref();
        show(space, entry.space.is_some());
        if let Some((fraction, text)) = &entry.space {
            imp.space.set_level(*fraction);
            space.set_value(none_if_empty(text));
            let tone = if !entry.problem.is_empty() || *fraction >= 1.0 {
                RowTone::Warning
            } else {
                RowTone::Normal
            };
            if space.tone() != tone {
                space.set_tone(tone);
            }
        }

        imp.facts.set_facts(&entry.facts);
        if imp.facts.get_visible() == entry.facts.is_empty() {
            imp.facts.set_visible(!entry.facts.is_empty());
        }

        let primary: &Row = imp.primary.upcast_ref();
        show(primary, entry.primary.is_some());
        if let Some(kind) = entry.primary {
            primary.set_title(Some(match kind {
                Primary::Open => gettextrs::gettext("Open"),
                Primary::Mount => gettextrs::gettext("Mount"),
            }));
        }

        let release: &Row = imp.release.upcast_ref();
        show(release, entry.release.is_some());
        match &entry.release {
            Some(Release::Eject(_)) => release.set_title(Some(gettextrs::gettext("Eject"))),
            Some(Release::Unmount(_)) => release.set_title(Some(gettextrs::gettext("Unmount"))),
            None => {}
        }
        imp.release_action.replace(entry.release.clone());

        let drawer = entry.space.is_some()
            || !entry.facts.is_empty()
            || entry.primary.is_some()
            || entry.release.is_some();
        if row.get_sensitive() != drawer {
            row.set_sensitive(drawer);
        }
        if !drawer {
            row.set_open(false);
        }
    }
}
