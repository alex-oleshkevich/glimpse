mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Row, SplitRow, SwitchRow, drawer, none_if_empty, reconcile, set_footer_row};

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
        self.render_details();
    }

    pub fn set_scanning(&self, scanning: bool) {
        let imp = self.imp();
        imp.search.set_active(scanning);
        if imp.nearby.get_visible() == scanning {
            return;
        }
        imp.nearby.set_visible(scanning);
        self.render_details();
    }

    pub fn set_details(&self, details: Option<&Details>) {
        let imp = self.imp();
        if imp.details.borrow().as_ref() == details {
            return;
        }
        imp.details.replace(details.cloned());
        self.render_details();
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

    pub fn set_discoverable(&self, on: bool) {
        self.imp().discoverable.set_active(on);
    }

    pub fn set_controls_sensitive(&self, on: bool) {
        let imp = self.imp();
        imp.search.set_sensitive(on);
        imp.discoverable.set_sensitive(on);
    }

    pub fn connect_discoverable<F: Fn(&Self, bool) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "discoverable",
            false,
            glib::closure_local!(move |popover: Self, on: bool| f(&popover, on)),
        )
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

    pub fn connect_activated<F: Fn(&Self, &str, bool) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |popover: Self, id: String, connected: bool| f(
                &popover, &id, connected
            )),
        )
    }

    pub fn connect_selected<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "selected",
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

    pub fn connect_scanning<F: Fn(&Self, bool) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "scanning",
            false,
            glib::closure_local!(move |popover: Self, wanted: bool| f(&popover, wanted)),
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

    fn place(&self, id: &str) -> Option<Place> {
        self.imp()
            .entries
            .borrow()
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.place)
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

        imp.pages.set_visible_child_name(PROMPT_PAGE);
        imp.hero.set_sensitive(false);
        imp.footer.set_sensitive(false);
    }

    fn render_entries(&self) {
        let imp = self.imp();
        let entries = imp.entries.borrow();

        for (place, section, parent, held) in [
            (
                Place::Connected,
                &imp.connected,
                &imp.connected_rows,
                &imp.connected_held,
            ),
            (
                Place::Paired,
                &imp.paired,
                &imp.paired_rows,
                &imp.paired_held,
            ),
        ] {
            let wanted: Vec<Entry> = entries
                .iter()
                .filter(|entry| entry.place == place)
                .cloned()
                .collect();
            reconcile::by_key(
                &**parent,
                &mut held.borrow_mut(),
                &wanted,
                |entry| entry.id.clone(),
                |entry| drawer::holder(&self.build_split(&entry.id)),
                |holder, entry| {
                    if let Some(split) = drawer::head::<SplitRow>(holder) {
                        dress_row(&split.row(), entry);
                    }
                },
            );
            section.set_visible(!wanted.is_empty());
        }

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
            |entry| drawer::holder(&self.build_nearby(&entry.id)),
            |holder, entry| {
                if let Some(row) = drawer::head::<Row>(holder) {
                    dress_row(&row, entry);
                }
            },
        );
        imp.nearby.set_visible(imp.search.active());
        imp.nearby.set_empty(nearby.is_empty());
    }

    fn render_details(&self) {
        let details = self.imp().details.borrow().clone();
        let wanted = details.as_ref().map(|details| details.id.as_str());
        let open = self
            .listed()
            .into_iter()
            .find(|(id, _)| wanted == Some(id.as_str()));

        for (id, holder) in self.holders() {
            if let Some(panel) = drawer::panel(&holder) {
                drawer::set(&panel, open.as_ref().is_some_and(|(open, _)| *open == id));
            }
        }

        if let (Some(details), Some((_, holder))) = (details.as_ref(), open.as_ref()) {
            self.fill(holder, details);
        }

        self.recede(open.as_ref().map(|(id, _)| id.as_str()));
    }

    /// The panel is built the first time its device is opened: a list of fourteen devices would
    /// otherwise carry fourteen row boxes that nothing has asked to see.
    fn fill(&self, holder: &gtk4::Box, details: &Details) {
        let Some(panel) = drawer::panel(holder) else {
            return;
        };
        let rows = match panel.child().and_downcast::<gtk4::Box>() {
            Some(rows) => rows,
            None => {
                let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                rows.add_css_class(DETAIL);
                panel.set_child(Some(&rows));
                rows
            }
        };
        let id = details.id.clone();
        let key = id.clone();
        reconcile::by_key(
            &rows,
            &mut self.imp().lines.borrow_mut(),
            &details.lines,
            |line| format!("{key}/{}", line.action),
            |line| self.build_line(&id, line),
            |row, line| self.dress_line(row, line),
        );
    }

    /// Everything but the open device recedes, so the panel is read against a quiet card rather
    /// than against a list that still looks clickable.
    fn recede(&self, open: Option<&str>) {
        let imp = self.imp();
        for (id, holder) in self.holders() {
            let Some(head) = holder.first_child() else {
                continue;
            };
            crate::set_css_class(&head, drawer::RECEDED, open.is_some_and(|open| open != id));
            crate::set_css_class(&head, drawer::OPEN, open == Some(id.as_str()));
        }
        for widget in [
            imp.search.upcast_ref::<gtk4::Widget>(),
            imp.discoverable.upcast_ref(),
            imp.more_paired.upcast_ref(),
            imp.more_nearby.upcast_ref(),
            imp.footer.upcast_ref(),
        ] {
            crate::set_css_class(widget, drawer::RECEDED, open.is_some());
        }
        crate::set_css_class(&*imp.hero, drawer::RECEDED, open.is_some());
    }

    fn listed(&self) -> Vec<(String, gtk4::Box)> {
        let imp = self.imp();
        let mut listed = Vec::new();
        for (section, held) in [
            (&imp.connected, &imp.connected_held),
            (&imp.paired, &imp.paired_held),
            (&imp.nearby, &imp.nearby_held),
        ] {
            if section.get_visible() && !section.empty() {
                listed.extend(held.borrow().iter().cloned());
            }
        }
        listed
    }

    fn holders(&self) -> impl Iterator<Item = (String, gtk4::Box)> + use<> {
        let imp = self.imp();
        let mut all = imp.connected_held.borrow().clone();
        all.extend(imp.paired_held.borrow().iter().cloned());
        all.extend(imp.nearby_held.borrow().iter().cloned());
        all.into_iter()
    }

    fn build_split(&self, id: &str) -> SplitRow {
        let split = SplitRow::new();
        split.add_css_class(DEVICE);
        let key = id.to_owned();
        split.connect_activated(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| {
                let connected = popover.place(&key) == Some(Place::Connected);
                popover.emit_by_name::<()>("activated", &[&key, &connected]);
            }
        ));
        let key = id.to_owned();
        split.connect_details(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>("selected", &[&key])
        ));
        split
    }

    fn build_nearby(&self, id: &str) -> Row {
        let row = Row::new();
        row.add_css_class(DEVICE);
        row.set_activatable(true);
        let key = id.to_owned();
        row.connect_clicked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>("selected", &[&key])
        ));
        row
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

const DETAIL: &str = "detail-card";
const DEVICE: &str = "bluetooth-popover__device";

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
    row.set_selected(entry.selected);
}
