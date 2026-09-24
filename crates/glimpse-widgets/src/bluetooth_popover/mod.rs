mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{
    Expandable, Row, SplitRow, SwitchRow, drawer, none_if_empty, reconcile, set_footer_row,
};

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

    pub fn set_adapter(&self, title: &str, subtitle: &str, icon: &str, on: bool, settable: bool) {
        let imp = self.imp();
        imp.hero.set_title(none_if_empty(title));
        imp.hero.set_subtitle(none_if_empty(subtitle));
        imp.hero.set_icon_name(none_if_empty(icon));

        imp.quiet.set(true);
        if imp.power.is_active() != on {
            imp.power.set_active(on);
        }
        imp.quiet.set(false);
        if imp.power.is_sensitive() != settable {
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

    /// Every listed device's detail, keyed by `Details::id`. A card is built the first time its
    /// device is opened and refreshed from here afterwards; a device whose detail is gone closes.
    pub fn set_details(&self, details: &[Details]) {
        let imp = self.imp();
        if imp.details.borrow().as_slice() == details {
            return;
        }
        imp.details.replace(details.to_vec());
        for (id, holder) in self.holders() {
            if holder.details::<gtk4::Widget>().is_some() {
                self.fill(&id, &holder);
            }
        }
    }

    /// Closes one device's card: what an applet calls once the action taken from it succeeded.
    pub fn collapse(&self, id: &str) {
        if let Some((_, holder)) = self.holders().find(|(held, _)| held == id) {
            holder.set_expanded(false);
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
        set_footer_row(&imp.more_paired, paired);
        set_footer_row(&imp.more_nearby, nearby);
    }

    pub fn connect_expanded<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "expanded",
            false,
            glib::closure_local!(move |popover: Self, place: String| f(&popover, &place)),
        )
    }

    /// Nearby devices, the same disclosure as the network popover's Other networks: the header
    /// shows once anything has been found and turns its chevron while `open`.
    pub fn set_nearby(&self, count: usize, open: bool) {
        let imp = self.imp();
        if imp.nearby.get_visible() != (count > 0) {
            imp.nearby.set_visible(count > 0);
        }
        crate::set_css_class(&*imp.search, drawer::OPEN, open);
        drawer::set(&imp.nearby_list, open);
    }

    pub fn connect_nearby_toggled<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "nearby-toggled",
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }

    pub fn set_controls_sensitive(&self, on: bool) {
        let imp = self.imp();
        imp.search.set_sensitive(on);
    }

    pub fn set_footer(&self, label: Option<&str>) {
        crate::set_footer_row(&self.imp().footer, label);
    }

    pub fn connect_powered<F: Fn(&Self, bool) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "powered",
            false,
            glib::closure_local!(move |popover: Self, on: bool| f(&popover, on)),
        )
    }

    /// A device not in use was pressed: connect it if it is paired, pair it if it is not. A device
    /// in use opens its card instead, and never reports here.
    pub fn connect_activated<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |popover: Self, id: String| f(&popover, &id)),
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

    pub fn connect_footer_activated<F: Fn(&Self) + 'static>(
        &self,
        handler: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "footer-activated",
            false,
            glib::closure_local!(move |popover: Self| handler(&popover)),
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
            imp.hero.set_sensitive(true);
            imp.footer.set_sensitive(true);
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

        for (_, holder) in self.holders() {
            holder.set_expanded(false);
        }
        imp.pages.set_visible_child_name(PROMPT_PAGE);
        imp.hero.set_sensitive(false);
        imp.footer.set_sensitive(false);
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
            |entry| Expandable::new(&self.head_for(entry)),
            |holder, entry| self.apply(holder, entry),
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
            |entry| Expandable::new(&self.head_for(entry)),
            |holder, entry| self.apply(holder, entry),
        );

        let live: Vec<String> = entries.iter().map(|entry| entry.id.clone()).collect();
        imp.lines.borrow_mut().retain(|id, _| live.contains(id));
    }

    /// A device in use is about that connection: the whole row opens its card, where Disconnect
    /// lives. A paired device connects on its body and opens its card from the chevron; a nearby
    /// one pairs on its body and has no card, because there is nothing to manage before it pairs.
    fn head_for(&self, entry: &Entry) -> gtk4::Widget {
        if entry.place == Place::Connected {
            let row = Row::new();
            row.add_css_class(DEVICE);
            let chevron = gtk4::Image::from_icon_name("go-next-symbolic");
            chevron.set_accessible_role(gtk4::AccessibleRole::Presentation);
            chevron.add_css_class("drawer-chevron");
            row.set_trail(&chevron);
            let key = entry.id.clone();
            row.connect_clicked(glib::clone!(
                #[weak(rename_to = popover)]
                self,
                move |_| popover.open(&key)
            ));
            return row.upcast();
        }

        let split = SplitRow::new();
        split.add_css_class(DEVICE);
        split.set_detail_visible(entry.place == Place::Paired);
        let key = entry.id.clone();
        split.connect_activated(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>("activated", &[&key])
        ));
        let key = entry.id.clone();
        split.connect_details(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.open(&key)
        ));
        split.upcast()
    }

    fn apply(&self, holder: &Expandable, entry: &Entry) {
        let connected = entry.place == Place::Connected;
        if connected == holder.head::<SplitRow>().is_some() {
            holder.set_head(&self.head_for(entry));
        }
        match holder.head::<SplitRow>() {
            Some(split) => {
                dress_row(&split.row(), entry);
                split.set_detail_visible(entry.place == Place::Paired);
            }
            None => {
                if let Some(row) = holder.head::<Row>() {
                    dress_row(&row, entry);
                }
            }
        }
    }

    fn open(&self, id: &str) {
        if let Some((_, holder)) = self.holders().find(|(held, _)| held == id)
            && holder.details::<gtk4::Widget>().is_none()
        {
            self.fill(id, &holder);
        }
    }

    fn fill(&self, id: &str, holder: &Expandable) {
        let imp = self.imp();
        let Some(details) = imp
            .details
            .borrow()
            .iter()
            .find(|details| details.id == id)
            .cloned()
        else {
            holder.set_details(None::<&gtk4::Widget>);
            return;
        };
        let rows = holder.details::<gtk4::Box>().unwrap_or_else(|| {
            let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            holder.set_details(Some(&rows));
            rows
        });
        let mut lines = imp.lines.borrow_mut();
        reconcile::by_key(
            &rows,
            lines.entry(id.to_owned()).or_default(),
            &details.lines,
            |line| line.action.clone(),
            |line| self.build_line(id, line),
            |row, line| self.dress_line(row, line),
        );
    }

    fn holders(&self) -> impl Iterator<Item = (String, Expandable)> + use<> {
        let imp = self.imp();
        let mut all = imp.devices_held.borrow().clone();
        all.extend(imp.nearby_held.borrow().iter().cloned());
        all.into_iter()
    }

    fn dress_line(&self, row: &Row, line: &Line) {
        row.set_title(none_if_empty(&line.title));
        row.set_value(none_if_empty(&line.value));
        row.set_busy(line.busy);

        let Some(on) = line.toggle else {
            return;
        };
        if let Some(toggle) = row.downcast_ref::<SwitchRow>() {
            toggle.set_active(on);
        }
    }

    fn build_line(&self, id: &str, line: &Line) -> Row {
        if let Some(on) = line.toggle {
            let key = id.to_owned();
            let action = line.action.clone();
            let toggle = SwitchRow::new();
            toggle.set_active(on);
            toggle.connect_toggled(glib::clone!(
                #[weak(rename_to = popover)]
                self,
                move |_, on| popover.emit_by_name::<()>("toggled", &[&key, &action, &on])
            ));
            return toggle.upcast();
        }

        let row = Row::new();
        row.set_activatable(line.activates);
        if !line.activates {
            return row;
        }
        let key = id.to_owned();
        let action = line.action.clone();
        row.connect_clicked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>("acted", &[&key, &action])
        ));
        row
    }
}

const DEVICE: &str = "bluetooth-popover__device";
const WARNING: &str = "bluetooth-popover__device--warning";

fn set_button(button: &gtk4::Button, label: &str) {
    button.set_visible(!label.is_empty());
    if label.is_empty() || button.label().is_some_and(|current| current == label) {
        return;
    }
    button.set_label(label);
}

fn dress_row(row: &Row, entry: &Entry) {
    row.set_title(none_if_empty(&entry.title));
    row.set_subtitle(none_if_empty(&entry.subtitle));
    row.set_lead_icon(none_if_empty(&entry.icon));
    row.set_value(none_if_empty(&entry.value));
    row.set_busy(entry.busy);
    crate::set_css_class(row, WARNING, entry.warning);
}
