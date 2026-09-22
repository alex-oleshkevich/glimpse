mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Row, SplitRow, none_if_empty, reconcile, set_css_class, set_footer_row};

pub use imp::{Drive, Volume};

const EJECT_ICON: &str = "media-eject-symbolic";
const READ_ONLY_ICON: &str = "changes-prevent-symbolic";
const DIMMED: &str = "dimmed";
const CAPACITY_BAR: &str = "capacity-bar";

enum Trail {
    None,
    ReadOnly,
    Eject(String),
    Unmount(String),
}

glib::wrapper! {
    pub struct RemovablePopover(ObjectSubclass<imp::RemovablePopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for RemovablePopover {
    fn default() -> Self {
        Self::new()
    }
}

struct DeviceRow {
    id: String,
    title: String,
    subtitle: String,
    icon: String,
    value: String,
    fraction: Option<f64>,
    activatable: bool,
    busy: bool,
    dimmed: bool,
    trail: Trail,
    indent: bool,
}

impl RemovablePopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_devices(&self, drives: &[Drive]) {
        let imp = self.imp();
        if imp.devices_data.borrow().as_slice() == drives {
            return;
        }
        imp.devices_data.replace(drives.to_vec());
        self.render_devices();
    }

    pub fn set_overflow(&self, devices: Option<&str>) {
        set_footer_row(&self.imp().devices_more, devices);
    }

    pub fn set_footer(&self, label: Option<&str>) {
        set_footer_row(&self.imp().footer, label);
    }

    pub fn connect_activated<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |popover: Self, id: String| f(&popover, &id)),
        )
    }

    pub fn connect_more<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "more",
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }

    pub fn connect_eject<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "eject",
            false,
            glib::closure_local!(move |popover: Self, id: String| f(&popover, &id)),
        )
    }

    pub fn connect_unmount<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "unmount",
            false,
            glib::closure_local!(move |popover: Self, id: String| f(&popover, &id)),
        )
    }

    pub fn connect_footer_activated<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "footer-activated",
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }

    fn render_devices(&self) {
        let imp = self.imp();
        let drives = imp.devices_data.borrow().clone();
        let rows = flatten_devices(&drives);
        imp.devices.set_visible(!rows.is_empty());
        reconcile::by_key(
            &*imp.devices_rows,
            &mut imp.devices_held.borrow_mut(),
            &rows,
            |row| row.id.clone(),
            |_row| self.build_device_cell(),
            |cell, spec| self.apply_device_cell(cell, spec),
        );
    }

    fn build_device_cell(&self) -> gtk4::Box {
        gtk4::Box::new(gtk4::Orientation::Vertical, 0)
    }

    fn apply_device_cell(&self, cell: &gtk4::Box, spec: &DeviceRow) {
        let split = matches!(spec.trail, Trail::Eject(_) | Trail::Unmount(_));
        let row = self.ensure_device_body(cell, spec, split);

        row.set_title(none_if_empty(&spec.title));
        row.set_subtitle(none_if_empty(&spec.subtitle));
        row.set_lead_icon(none_if_empty(&spec.icon));
        row.set_value(none_if_empty(&spec.value));
        row.set_busy(spec.busy);
        row.set_activatable(spec.activatable);
        set_css_class(&row, DIMMED, spec.dimmed);

        let margin = if spec.indent { 24 } else { 0 };
        if cell.margin_start() != margin {
            cell.set_margin_start(margin);
        }

        if !split {
            let wanted_icon = matches!(spec.trail, Trail::ReadOnly).then_some(READ_ONLY_ICON);
            if let Some(trail) = row.trail().and_downcast::<gtk4::Image>() {
                if trail.icon_name().as_deref() != wanted_icon {
                    trail.set_icon_name(wanted_icon);
                }
                if trail.get_visible() != wanted_icon.is_some() {
                    trail.set_visible(wanted_icon.is_some());
                }
            }
        }

        apply_capacity_bar(cell, spec.fraction);
    }

    fn ensure_device_body(&self, cell: &gtk4::Box, spec: &DeviceRow, split: bool) -> Row {
        if split {
            if let Some(existing) = cell.first_child().and_downcast::<SplitRow>() {
                return existing.row();
            }
            if let Some(old) = cell.first_child() {
                old.unparent();
            }
            let split_row = SplitRow::new();
            let key = spec.id.clone();
            split_row.connect_activated(glib::clone!(
                #[weak(rename_to = popover)]
                self,
                move |_| popover.emit_by_name::<()>("activated", &[&key])
            ));
            match &spec.trail {
                Trail::Eject(drive_id) => {
                    split_row.set_detail_icon(EJECT_ICON.to_owned());
                    split_row.set_detail_tooltip(Some(gettextrs::gettext("Eject")));
                    let drive_id = drive_id.clone();
                    split_row.connect_details(glib::clone!(
                        #[weak(rename_to = popover)]
                        self,
                        move |_| popover.emit_by_name::<()>("eject", &[&drive_id])
                    ));
                }
                Trail::Unmount(volume_id) => {
                    split_row.set_detail_icon(EJECT_ICON.to_owned());
                    split_row.set_detail_tooltip(Some(gettextrs::gettext("Unmount")));
                    let volume_id = volume_id.clone();
                    split_row.connect_details(glib::clone!(
                        #[weak(rename_to = popover)]
                        self,
                        move |_| popover.emit_by_name::<()>("unmount", &[&volume_id])
                    ));
                }
                Trail::None | Trail::ReadOnly => {}
            }
            cell.prepend(&split_row);
            split_row.row()
        } else {
            if let Some(existing) = cell.first_child().and_downcast::<Row>() {
                return existing;
            }
            if let Some(old) = cell.first_child() {
                old.unparent();
            }
            let row = Row::new();
            let trail = gtk4::Image::new();
            trail.set_accessible_role(gtk4::AccessibleRole::Presentation);
            trail.set_visible(false);
            row.set_trail(&trail);
            let key = spec.id.clone();
            row.connect_clicked(glib::clone!(
                #[weak(rename_to = popover)]
                self,
                move |_| popover.emit_by_name::<()>("activated", &[&key])
            ));
            cell.prepend(&row);
            row
        }
    }
}

fn apply_capacity_bar(cell: &gtk4::Box, fraction: Option<f64>) {
    match fraction {
        Some(fraction) => {
            let bar = match cell.last_child().and_downcast::<gtk4::ProgressBar>() {
                Some(bar) => bar,
                None => {
                    let bar = gtk4::ProgressBar::new();
                    bar.add_css_class(CAPACITY_BAR);
                    bar.set_margin_start(44);
                    bar.set_margin_end(12);
                    bar.set_margin_bottom(6);
                    cell.append(&bar);
                    bar
                }
            };
            if bar.fraction() != fraction {
                bar.set_fraction(fraction);
            }
        }
        None => {
            if let Some(bar) = cell.last_child().and_downcast::<gtk4::ProgressBar>() {
                bar.unparent();
            }
        }
    }
}

fn trail_for(busy: bool, read_only: bool, eject: Option<&str>, unmount: Option<&str>) -> Trail {
    if busy {
        return Trail::None;
    }
    if read_only {
        return Trail::ReadOnly;
    }
    if let Some(id) = eject {
        return Trail::Eject(id.to_owned());
    }
    if let Some(id) = unmount {
        return Trail::Unmount(id.to_owned());
    }
    Trail::None
}

fn flatten_devices(drives: &[Drive]) -> Vec<DeviceRow> {
    let mut rows = Vec::new();
    for drive in drives {
        match drive.volumes.as_slice() {
            [] => rows.push(DeviceRow {
                id: drive.id.clone(),
                title: drive.title.clone(),
                subtitle: drive.subtitle.clone(),
                icon: drive.icon.clone(),
                value: drive.value.clone(),
                fraction: None,
                activatable: drive.activatable,
                busy: drive.busy,
                dimmed: drive.dimmed,
                trail: trail_for(
                    drive.busy,
                    false,
                    drive.ejectable.then_some(drive.id.as_str()),
                    None,
                ),
                indent: false,
            }),
            [volume] => rows.push(DeviceRow {
                id: drive.id.clone(),
                title: volume.title.clone(),
                subtitle: volume.subtitle.clone(),
                icon: volume.icon.clone(),
                value: volume.value.clone(),
                fraction: volume.fraction,
                activatable: volume.activatable,
                busy: drive.busy || volume.busy,
                dimmed: false,
                trail: trail_for(
                    drive.busy || volume.busy,
                    volume.read_only,
                    drive.ejectable.then_some(drive.id.as_str()),
                    volume.mounted.then_some(volume.id.as_str()),
                ),
                indent: false,
            }),
            volumes => {
                rows.push(DeviceRow {
                    id: drive.id.clone(),
                    title: drive.title.clone(),
                    subtitle: drive.subtitle.clone(),
                    icon: drive.icon.clone(),
                    value: drive.value.clone(),
                    fraction: None,
                    activatable: drive.activatable,
                    busy: drive.busy,
                    dimmed: drive.dimmed,
                    trail: trail_for(
                        drive.busy,
                        false,
                        drive.ejectable.then_some(drive.id.as_str()),
                        None,
                    ),
                    indent: false,
                });
                for volume in volumes {
                    rows.push(DeviceRow {
                        id: format!("{}/{}", drive.id, volume.id),
                        title: volume.title.clone(),
                        subtitle: volume.subtitle.clone(),
                        icon: volume.icon.clone(),
                        value: volume.value.clone(),
                        fraction: volume.fraction,
                        activatable: volume.activatable,
                        busy: volume.busy,
                        dimmed: false,
                        trail: trail_for(
                            volume.busy,
                            volume.read_only,
                            None,
                            volume.mounted.then_some(volume.id.as_str()),
                        ),
                        indent: true,
                    });
                }
            }
        }
    }
    rows
}
