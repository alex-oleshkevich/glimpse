mod device;
mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{reconcile, set_action_label};

pub(crate) use device::RemovableDevice;

pub use imp::{Drive, Space, Volume};

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

    fn render_devices(&self) {
        let imp = self.imp();
        let (drives, rest) = imp.devices_data.borrow().clone();
        let any = !drives.is_empty() || !rest.is_empty();
        if imp.devices.get_visible() != any {
            imp.devices.set_visible(any);
        }
        reconcile::by_key(
            &*imp.devices_rows,
            &mut imp.devices_held.borrow_mut(),
            &drives,
            |drive| drive.id.clone(),
            |drive| self.build_device(drive),
            |device, drive| device.set_entry(drive),
        );
        reconcile::by_key(
            &*imp.more_rows,
            &mut imp.more_held.borrow_mut(),
            &rest,
            |drive| drive.id.clone(),
            |drive| self.build_device(drive),
            |device, drive| device.set_entry(drive),
        );
    }

    fn build_device(&self, _drive: &Drive) -> RemovableDevice {
        let device: RemovableDevice = glib::Object::new();
        device.set_owner(self);
        device.imp().primary.connect_activated(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            #[weak]
            device,
            move |_| {
                let target = device.imp().open.borrow().clone();
                if let Some(id) = target {
                    popover.emit_by_name::<()>("activated", &[&id]);
                }
            }
        ));
        device.imp().release.connect_activated(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            #[weak]
            device,
            move |_| {
                let id = device.imp().drive.borrow().clone();
                popover.emit_by_name::<()>("eject", &[&id]);
            }
        ));
        device
    }
}
