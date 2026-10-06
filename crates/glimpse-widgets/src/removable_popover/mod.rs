mod device;
mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{reconcile, set_action_label};

use device::{Entry, Primary, Release};

pub(crate) use device::RemovableDevice;

pub use imp::{Drive, Volume};

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

impl RemovablePopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_devices(&self, drives: &[Drive], rest: &[Drive]) {
        let imp = self.imp();
        let wanted = (drives.to_vec(), rest.to_vec());
        if *imp.devices_data.borrow() == wanted {
            return;
        }
        imp.devices_data.replace(wanted);
        self.render_devices();
    }

    pub fn set_overflow(&self, title: Option<&str>, open: bool) {
        let imp = self.imp();
        let header = &imp.devices_more;
        header.set_title(title);
        if header.get_visible() != title.is_some() {
            header.set_visible(title.is_some());
        }
        imp.quiet.set(true);
        header.set_open(title.is_some() && open);
        imp.quiet.set(false);
    }

    pub fn set_footer(&self, label: Option<&str>) {
        set_action_label(&self.imp().footer, label);
    }

    pub fn connect_activated<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |popover: Self, id: String| f(&popover, &id)),
        )
    }

    pub fn connect_more<F: Fn(&Self, bool) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "more",
            false,
            glib::closure_local!(move |popover: Self, open: bool| f(&popover, open)),
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
        let (drives, rest) = imp.devices_data.borrow().clone();
        let entries = flatten_devices(&drives);
        let more = flatten_devices(&rest);
        let any = !entries.is_empty() || !more.is_empty();
        if imp.devices.get_visible() != any {
            imp.devices.set_visible(any);
        }
        reconcile::by_key(
            &*imp.devices_rows,
            &mut imp.devices_held.borrow_mut(),
            &entries,
            |entry| entry.id.clone(),
            |entry| self.build_device(entry),
            |device, entry| device.set_entry(entry),
        );
        reconcile::by_key(
            &*imp.more_rows,
            &mut imp.more_held.borrow_mut(),
            &more,
            |entry| entry.id.clone(),
            |entry| self.build_device(entry),
            |device, entry| device.set_entry(entry),
        );
    }

    fn build_device(&self, entry: &Entry) -> RemovableDevice {
        let device: RemovableDevice = glib::Object::new();
        let key = entry.id.clone();
        device.imp().primary.connect_activated(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>("activated", &[&key])
        ));
        device.imp().release.connect_activated(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            #[weak]
            device,
            move |_| match device.imp().release_action.borrow().clone() {
                Some(Release::Eject(id)) => popover.emit_by_name::<()>("eject", &[&id]),
                Some(Release::Unmount(id)) => popover.emit_by_name::<()>("unmount", &[&id]),
                None => {}
            }
        ));
        device
    }
}

fn release(drive: &Drive, volume: Option<&Volume>) -> Option<Release> {
    if drive.ejectable {
        return Some(Release::Eject(drive.id.clone()));
    }
    volume
        .filter(|volume| volume.mounted)
        .map(|volume| Release::Unmount(volume.id.clone()))
}

fn drive_entry(drive: &Drive) -> Entry {
    Entry {
        id: drive.id.clone(),
        title: drive.title.clone(),
        icon: drive.icon.clone(),
        busy: drive.busy,
        inactive: drive.dimmed,
        problem: String::new(),
        space: None,
        facts: drive.facts.clone(),
        primary: None,
        release: release(drive, None).filter(|_| !drive.busy && !drive.dimmed),
    }
}

fn volume_entry(id: String, drive: &Drive, volume: &Volume) -> Entry {
    let busy = drive.busy || volume.busy;
    Entry {
        id,
        title: volume.title.clone(),
        icon: volume.icon.clone(),
        busy,
        inactive: !volume.mounted,
        problem: volume.problem.clone(),
        space: volume
            .fraction
            .filter(|_| volume.mounted)
            .map(|fraction| (fraction, volume.space.clone())),
        facts: volume.facts.clone(),
        primary: Some(if volume.mounted {
            Primary::Open
        } else {
            Primary::Mount
        })
        .filter(|_| !busy),
        release: release(drive, Some(volume)).filter(|_| !busy),
    }
}

fn flatten_devices(drives: &[Drive]) -> Vec<Entry> {
    let mut entries = Vec::new();
    for drive in drives {
        match drive.volumes.as_slice() {
            [] => entries.push(drive_entry(drive)),
            volumes => {
                entries.extend(volumes.iter().map(|volume| {
                    volume_entry(format!("{}/{}", drive.id, volume.id), drive, volume)
                }))
            }
        }
    }
    entries
}
