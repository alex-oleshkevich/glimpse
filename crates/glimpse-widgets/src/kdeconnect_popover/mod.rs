mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{ActionRow, ExpanderRow, Row, RowTone, none_if_empty, reconcile, set_action_label};

pub use imp::{Action, Device, Nearby};

glib::wrapper! {
    pub struct KdeconnectPopover(ObjectSubclass<imp::KdeconnectPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for KdeconnectPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl KdeconnectPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_devices(&self, devices: &[Device]) {
        let imp = self.imp();
        if imp.devices_data.borrow().as_slice() == devices {
            return;
        }
        let previous = imp.devices_data.replace(devices.to_vec());
        imp.devices.set_visible(!devices.is_empty());
        reconcile::by_key(
            &*imp.devices_rows,
            &mut imp.devices_held.borrow_mut(),
            devices,
            |device| device.id.clone(),
            |_| ExpanderRow::new(),
            |holder, device| {
                let unchanged = previous
                    .iter()
                    .find(|old| old.id == device.id)
                    .is_some_and(|old| old.actions == device.actions);
                self.apply_device(holder, device, !unchanged);
            },
        );
    }

    pub fn set_nearby(&self, nearby: &[Nearby], more: Option<&str>) {
        let imp = self.imp();
        set_action_label(&imp.nearby_more, more);
        imp.nearby.set_visible(!nearby.is_empty());
        if nearby.is_empty() {
            imp.nearby_toggle.set_open(false);
        }
        reconcile::by_key(
            &*imp.nearby_rows,
            &mut imp.nearby_held.borrow_mut(),
            nearby,
            |entry| entry.id.clone(),
            |entry| self.nearby_row(&entry.id),
            apply_nearby,
        );
    }

    pub fn set_nearby_open(&self, open: bool) {
        self.imp().nearby_toggle.set_open(open);
    }

    pub fn nearby_open(&self) -> bool {
        self.imp().nearby_toggle.open()
    }

    pub fn collapse(&self, id: &str) {
        let held = self.imp().devices_held.borrow();
        if let Some((_, holder)) = held.iter().find(|(key, _)| key == id) {
            holder.set_open(false);
        }
    }

    pub fn set_footer(&self, label: Option<&str>) {
        set_action_label(&self.imp().footer, label);
    }

    pub fn connect_action<F: Fn(&Self, &str, &str) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "action",
            false,
            glib::closure_local!(move |popover: Self, id: String, key: String| f(
                &popover, &id, &key
            )),
        )
    }

    pub fn connect_pair<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "pair",
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

    fn apply_device(&self, holder: &ExpanderRow, device: &Device, actions_changed: bool) {
        holder.set_title(none_if_empty(&device.title));
        holder.set_value(none_if_empty(&device.value));
        if !actions_changed {
            return;
        }
        let details = holder.details();
        while let Some(child) = details.first_child() {
            details.remove(&child);
        }
        for action in &device.actions {
            let row = ActionRow::new();
            let base = row.upcast_ref::<Row>();
            base.set_title(Some(action.label.as_str()));
            base.set_tone(if action.destructive {
                RowTone::Destructive
            } else {
                RowTone::Normal
            });
            let id = device.id.clone();
            let key = action.key.clone();
            row.connect_activated(glib::clone!(
                #[weak(rename_to = popover)]
                self,
                move |_| popover.emit_by_name::<()>("action", &[&id, &key])
            ));
            details.append(&row);
        }
    }

    fn nearby_row(&self, id: &str) -> ActionRow {
        let row = ActionRow::new();
        let id = id.to_owned();
        row.connect_activated(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |row| {
                let row = row.upcast_ref::<Row>();
                if row.busy() {
                    return;
                }
                row.set_busy(true);
                popover.emit_by_name::<()>("pair", &[&id]);
            }
        ));
        row
    }
}

fn apply_nearby(row: &ActionRow, entry: &Nearby) {
    let row = row.upcast_ref::<Row>();
    row.set_title(none_if_empty(&entry.title));
    row.set_subtitle(none_if_empty(&entry.subtitle));
    row.set_busy(entry.busy);
}
