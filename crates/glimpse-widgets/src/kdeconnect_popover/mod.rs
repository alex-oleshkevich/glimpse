mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Expandable, Row, SplitRow, none_if_empty, reconcile, set_css_class, set_footer_row};

pub use imp::{Action, Device, Nearby};

const CHEVRON: &str = "go-next-symbolic";
const CHEVRON_CLASS: &str = "drawer-chevron";
const OPEN: &str = "open";

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

    pub fn set_summary(&self, summary: &str) {
        let hero = &self.imp().hero;
        if hero.subtitle().as_deref() != none_if_empty(summary) {
            hero.set_subtitle(none_if_empty(summary));
        }
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
            |_| self.device_row(),
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
        set_footer_row(&imp.nearby_more, more);
        imp.nearby.set_visible(!nearby.is_empty());
        reconcile::by_key(
            &*imp.nearby_rows,
            &mut imp.nearby_held.borrow_mut(),
            nearby,
            |entry| entry.id.clone(),
            |entry| self.nearby_row(&entry.id),
            apply_nearby,
        );
        self.sync_nearby();
    }

    pub fn set_nearby_open(&self, open: bool) {
        self.imp().nearby_open.set(open);
        self.sync_nearby();
    }

    fn sync_nearby(&self) {
        let imp = self.imp();
        let open = imp.nearby_open.get();
        set_css_class(&*imp.nearby_toggle, OPEN, open);
        if imp.nearby_rows.get_visible() != open {
            imp.nearby_rows.set_visible(open);
        }
        let more = open && imp.nearby_more.title().is_some();
        if imp.nearby_more.get_visible() != more {
            imp.nearby_more.set_visible(more);
        }
    }

    pub fn nearby_open(&self) -> bool {
        self.imp().nearby_open.get()
    }

    pub fn collapse(&self, id: &str) {
        let held = self.imp().devices_held.borrow();
        if let Some((_, holder)) = held.iter().find(|(key, _)| key == id) {
            holder.set_expanded(false);
        }
    }

    pub fn set_footer(&self, label: Option<&str>) {
        set_footer_row(&self.imp().footer, label);
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

    fn device_row(&self) -> Expandable {
        let head = Row::new();
        let chevron = gtk4::Image::from_icon_name(CHEVRON);
        chevron.add_css_class(CHEVRON_CLASS);
        chevron.set_accessible_role(gtk4::AccessibleRole::Presentation);
        head.set_trail(&chevron);
        Expandable::new(&head)
    }

    fn apply_device(&self, holder: &Expandable, device: &Device, actions_changed: bool) {
        let Some(head) = holder.head::<Row>() else {
            return;
        };
        head.set_title(none_if_empty(&device.title));
        head.set_subtitle(none_if_empty(&device.subtitle));
        head.set_value(none_if_empty(&device.value));

        let details = match holder.details::<gtk4::Box>() {
            Some(_) if !actions_changed => return,
            Some(details) => details,
            None => {
                let details = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                holder.set_details(Some(&details));
                details
            }
        };
        while let Some(child) = details.first_child() {
            details.remove(&child);
        }
        for action in &device.actions {
            let row = Row::new();
            row.set_title(Some(action.label.as_str()));
            let id = device.id.clone();
            let key = action.key.clone();
            row.connect_clicked(glib::clone!(
                #[weak(rename_to = popover)]
                self,
                move |_| popover.emit_by_name::<()>("action", &[&id, &key])
            ));
            details.append(&row);
        }
    }

    fn nearby_row(&self, id: &str) -> SplitRow {
        let split = SplitRow::new();
        split.set_detail_visible(false);
        let id = id.to_owned();
        split.connect_activated(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |split| {
                let row = split.row();
                if row.busy() {
                    return;
                }
                row.set_busy(true);
                popover.emit_by_name::<()>("pair", &[&id]);
            }
        ));
        split
    }
}

fn apply_nearby(split: &SplitRow, entry: &Nearby) {
    let row = split.row();
    row.set_title(none_if_empty(&entry.title));
    row.set_subtitle(none_if_empty(&entry.subtitle));
    row.set_busy(entry.busy);
}
