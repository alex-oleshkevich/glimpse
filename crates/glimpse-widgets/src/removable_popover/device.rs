use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{ActionRow, Row, RowTone, none_if_empty, reconcile};

use super::{Drive, Volume};

mod imp {
    use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};
    use std::cell::RefCell;

    use crate::{ActionRow, ExpanderRow, MeterRow};

    use super::super::RemovablePopover;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/me/aresa/GlimpseShell/widgets/removable_device.ui")]
    pub struct RemovableDevice {
        #[template_child]
        pub row: TemplateChild<ExpanderRow>,
        #[template_child]
        pub mounted: TemplateChild<gtk4::Image>,
        #[template_child]
        pub space: TemplateChild<MeterRow>,
        #[template_child]
        pub extras: TemplateChild<gtk4::Box>,
        #[template_child]
        pub primary: TemplateChild<ActionRow>,
        #[template_child]
        pub release: TemplateChild<ActionRow>,

        pub(crate) drive: RefCell<String>,
        pub(crate) open: RefCell<Option<String>>,
        pub(crate) held: RefCell<Vec<(String, ActionRow)>>,
        pub(crate) owner: glib::WeakRef<RemovablePopover>,
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

fn enable(row: &Row, sensitive: bool) {
    if row.get_sensitive() != sensitive {
        row.set_sensitive(sensitive);
    }
}

impl RemovableDevice {
    pub(super) fn set_owner(&self, owner: &super::RemovablePopover) {
        self.imp().owner.set(Some(owner));
    }

    pub(super) fn set_entry(&self, drive: &Drive) {
        let imp = self.imp();
        let row = &imp.row;
        imp.drive.replace(drive.id.clone());
        imp.open.replace(drive.open.clone());
        row.set_title(none_if_empty(&drive.title));
        row.set_icon_name(none_if_empty(&drive.icon));
        row.set_busy(drive.busy);
        if imp.mounted.get_visible() != drive.mounted {
            imp.mounted.set_visible(drive.mounted);
        }
        row.set_subtitle(none_if_empty(&drive.problem));
        row.set_tone(if drive.problem.is_empty() {
            RowTone::Normal
        } else {
            RowTone::Warning
        });

        let space: &Row = imp.space.upcast_ref();
        show(space, drive.space.is_some());
        if let Some(meter) = &drive.space {
            space.set_title(none_if_empty(&meter.title));
            imp.space.set_level(meter.fraction);
            space.set_value(none_if_empty(&meter.text));
            let tone = if !drive.problem.is_empty() || meter.fraction >= 1.0 {
                RowTone::Warning
            } else {
                RowTone::Normal
            };
            if space.tone() != tone {
                space.set_tone(tone);
            }
        }

        let idle = !drive.busy;
        let owner = imp.owner.clone();
        reconcile::by_key(
            &*imp.extras,
            &mut imp.held.borrow_mut(),
            &drive.volumes,
            |volume| volume.id.clone(),
            |volume| extra_row(volume, &owner),
            |row, volume| apply_extra(row, volume, idle),
        );
        let extras = !drive.volumes.is_empty();
        if imp.extras.get_visible() != extras {
            imp.extras.set_visible(extras);
        }

        let primary: &Row = imp.primary.upcast_ref();
        show(primary, drive.open.is_some());
        enable(primary, idle);

        let release: &Row = imp.release.upcast_ref();
        show(release, drive.removable);
        enable(release, idle);

        let drawer = drive.space.is_some() || extras || drive.open.is_some() || drive.removable;
        if row.get_sensitive() != drawer {
            row.set_sensitive(drawer);
        }
        if !drawer {
            row.set_open(false);
        }
    }
}

fn extra_row(volume: &Volume, owner: &glib::WeakRef<super::RemovablePopover>) -> ActionRow {
    let row = ActionRow::new();
    let key = volume.id.clone();
    let owner = owner.clone();
    row.connect_activated(move |_| {
        if let Some(popover) = owner.upgrade() {
            popover.emit_by_name::<()>("activated", &[&key]);
        }
    });
    row
}

fn apply_extra(row: &ActionRow, volume: &Volume, idle: bool) {
    let base: &Row = row.upcast_ref();
    base.set_title(none_if_empty(&volume.title));
    base.set_value(none_if_empty(&volume.size));
    enable(base, idle);
    let marked = base.imp().end().first_child().is_some();
    if volume.mounted && !marked {
        let check = gtk4::Image::from_icon_name("object-select-symbolic");
        check.add_css_class("accent");
        check.set_accessible_role(gtk4::AccessibleRole::Presentation);
        base.set_end(&check);
    } else if !volume.mounted && marked {
        base.clear_end();
    }
}
