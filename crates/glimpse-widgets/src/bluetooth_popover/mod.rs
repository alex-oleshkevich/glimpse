mod device;
mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{none_if_empty, reconcile, set_action_label};

pub(crate) use device::BluetoothDevice;

pub use imp::{Ask, Details, Entry, Line, Place};

const DEVICES_PAGE: &str = "devices";
const PROMPT_PAGE: &str = "prompt";
const BARE: &str = "prompt__actions--bare";
const ACCEPT: &str = "prompt__accept";
const DESTRUCTIVE: &str = "prompt__accept--danger";

glib::wrapper! {
    pub struct BluetoothPopover(ObjectSubclass<imp::BluetoothPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for BluetoothPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl BluetoothPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_adapter(&self, title: &str, on: bool, settable: bool) {
        let imp = self.imp();
        imp.shell.set_title(none_if_empty(title));

        imp.quiet.set(true);
        if imp.power.is_active() != on {
            imp.power.set_active(on);
        }
        imp.quiet.set(false);
        if imp.power.get_sensitive() != settable {
            imp.power.set_sensitive(settable);
        }
    }

    pub fn set_entries(&self, entries: &[Entry]) {
        let imp = self.imp();
        if imp.entries.borrow().as_slice() == entries {
            return;
        }
        imp.entries.replace(entries.to_vec());
        self.render_entries();
    }

    pub fn set_details(&self, details: &[Details]) {
        let imp = self.imp();
        imp.details.replace(details.to_vec());
        for (id, device) in self.devices() {
            device.set_details(details.iter().find(|details| details.id == id));
        }
    }

    pub fn collapse(&self, id: &str) {
        if let Some((_, device)) = self.devices().find(|(held, _)| held == id) {
            device.imp().row.set_open(false);
        }
    }

    pub fn set_prompt(&self, ask: Option<&Ask>) {
        let imp = self.imp();
        if imp.prompt.borrow().as_ref() == ask {
            return;
        }
        let held = imp.prompt.borrow().as_ref().map(|held| held.key.clone());
        let switched = held.as_deref() != ask.map(|next| next.key.as_str());
        imp.prompt.replace(ask.cloned());
        self.render_prompt(switched);
    }

    pub fn connect_answered<F: Fn(&Self, bool) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "answered",
            false,
            glib::closure_local!(move |popover: Self, accepted: bool| f(&popover, accepted)),
        )
    }

    pub fn set_overflow(&self, paired: Option<&str>, nearby: Option<&str>) {
        let imp = self.imp();
        set_action_label(&imp.more_paired, paired);
        set_action_label(&imp.more_nearby, nearby);
    }

    pub fn connect_expanded<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "expanded",
            false,
            glib::closure_local!(move |popover: Self, place: String| f(&popover, &place)),
        )
    }

    pub fn set_nearby(&self, count: usize, open: bool) {
        let imp = self.imp();
        if imp.nearby.get_visible() != (count > 0) {
            imp.nearby.set_visible(count > 0);
        }
        imp.quiet.set(true);
        imp.search
            .set_open(open && count > 0 && imp.prompt.borrow().is_none());
        imp.quiet.set(false);
    }

    pub fn connect_nearby_toggled<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "nearby-toggled",
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }

    pub fn set_controls_sensitive(&self, on: bool) {
        let search = &self.imp().search;
        if search.get_sensitive() != on {
            search.set_sensitive(on);
        }
    }

    pub fn connect_powered<F: Fn(&Self, bool) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "powered",
            false,
            glib::closure_local!(move |popover: Self, on: bool| f(&popover, on)),
        )
    }

    pub fn connect_toggled<F: Fn(&Self, &str, &str, bool) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "toggled",
            false,
            glib::closure_local!(
                move |popover: Self, id: String, action: String, on: bool| f(
                    &popover, &id, &action, on
                )
            ),
        )
    }

    pub fn connect_acted<F: Fn(&Self, &str, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "acted",
            false,
            glib::closure_local!(move |popover: Self, id: String, action: String| f(
                &popover, &id, &action
            )),
        )
    }

    fn render_prompt(&self, switched: bool) {
        let imp = self.imp();
        if switched {
            for button in [&imp.prompt_cancel, &imp.prompt_accept] {
                button.set_sensitive(false);
                button.set_sensitive(true);
            }
        }
        let prompt = imp.prompt.borrow();
        let Some(ask) = prompt.as_ref() else {
            imp.pages.set_visible_child_name(DEVICES_PAGE);
            imp.shell.imp().header.set_sensitive(true);
            return;
        };

        crate::set_text(&imp.prompt_device, none_if_empty(&ask.device));
        crate::set_text(&imp.prompt_ask, none_if_empty(&ask.question));
        crate::set_text(&imp.prompt_code, none_if_empty(&ask.code));
        crate::set_text(&imp.prompt_progress, none_if_empty(&ask.progress));
        set_button(&imp.prompt_cancel, &ask.cancel);
        set_button(&imp.prompt_accept, &ask.accept);
        crate::set_css_class(&*imp.prompt_accept, ACCEPT, true);
        crate::set_css_class(&*imp.prompt_accept, DESTRUCTIVE, ask.destructive);
        crate::set_css_class(&*imp.prompt_actions, BARE, ask.code.is_empty());

        for (_, device) in self.devices() {
            device.imp().row.set_open(false);
        }
        imp.quiet.set(true);
        imp.search.set_open(false);
        imp.quiet.set(false);
        imp.pages.set_visible_child_name(PROMPT_PAGE);
        imp.shell.imp().header.set_sensitive(false);
    }

    fn render_entries(&self) {
        let imp = self.imp();
        let entries = imp.entries.borrow().clone();

        let devices: Vec<Entry> = entries
            .iter()
            .filter(|entry| entry.place != Place::Nearby)
            .cloned()
            .collect();
        reconcile::by_key(
            &*imp.device_rows,
            &mut imp.devices_held.borrow_mut(),
            &devices,
            |entry| entry.id.clone(),
            |entry| self.build_device(entry),
            |device, entry| device.set_entry(entry),
        );
        imp.devices.set_visible(!devices.is_empty());

        let nearby: Vec<Entry> = entries
            .iter()
            .filter(|entry| entry.place == Place::Nearby)
            .cloned()
            .collect();
        reconcile::by_key(
            &*imp.nearby_rows,
            &mut imp.nearby_held.borrow_mut(),
            &nearby,
            |entry| entry.id.clone(),
            |entry| self.build_device(entry),
            |device, entry| device.set_entry(entry),
        );
    }

    fn build_device(&self, entry: &Entry) -> BluetoothDevice {
        let device: BluetoothDevice = glib::Object::new();
        let key = entry.id.clone();
        device.imp().primary.connect_activated(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            #[weak]
            device,
            move |_| {
                let action = device.imp().primary_action.borrow().clone();
                if !action.is_empty() {
                    popover.emit_by_name::<()>("acted", &[&key, &action]);
                }
            }
        ));
        let key = entry.id.clone();
        device.imp().trust.connect_toggled(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_, on| popover.emit_by_name::<()>("toggled", &[&key, &"trust", &on])
        ));
        let key = entry.id.clone();
        device.imp().forget.connect_activated(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>("acted", &[&key, &"forget"])
        ));
        let details = self.imp().details.borrow();
        device.set_details(details.iter().find(|details| details.id == entry.id));
        device
    }

    fn devices(&self) -> impl Iterator<Item = (String, BluetoothDevice)> + use<> {
        let imp = self.imp();
        let mut all = imp.devices_held.borrow().clone();
        all.extend(imp.nearby_held.borrow().iter().cloned());
        all.into_iter()
    }
}

fn set_button(button: &gtk4::Button, label: &str) {
    button.set_visible(!label.is_empty());
    if label.is_empty() || button.label().is_some_and(|current| current == label) {
        return;
    }
    button.set_label(label);
}
