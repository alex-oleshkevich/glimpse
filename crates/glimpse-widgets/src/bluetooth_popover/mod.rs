mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{
    Notice, Row, Severity, SplitRow, SwitchRow, drawer, none_if_empty, reconcile, set_footer_row,
};

pub use imp::{Details, Entry, Line, Place};

const SCAN_ICON: &str = "list-add-symbolic";
const STOP_ICON: &str = "process-stop-symbolic";

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

    pub fn set_scanning(&self, scanning: bool, label: &str) {
        let imp = self.imp();
        imp.scan.set_title(none_if_empty(label));
        imp.scan.set_lead_icon(Some(match scanning {
            true => STOP_ICON,
            false => SCAN_ICON,
        }));
        if imp.scanning.get() == scanning {
            return;
        }
        imp.scanning.set(scanning);
        imp.nearby.set_visible(scanning);
    }

    pub fn set_details(&self, details: Option<&Details>) {
        let imp = self.imp();
        if imp.details.borrow().as_ref() == details {
            return;
        }
        imp.details.replace(details.cloned());
        self.render_details();
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

    pub fn set_visible_as(&self, label: Option<&str>) {
        crate::set_text(&self.imp().visible_as, label);
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
        imp.nearby.set_visible(imp.scanning.get());
    }

    fn render_details(&self) {
        let details = self.imp().details.borrow().clone();
        let open = details.as_ref().map(|details| details.id.as_str());

        for (id, holder) in self.holders() {
            if let Some(panel) = drawer::panel(&holder) {
                drawer::set(&panel, open == Some(id.as_str()));
            }
        }

        if let Some(details) = details.as_ref()
            && let Some(holder) = self.holders().find(|(id, _)| *id == details.id)
        {
            self.fill(&holder.1, details);
        }

        self.recede(open);
    }

    /// The panel is built the first time its device is opened: a list of fourteen devices would
    /// otherwise carry fourteen notices and fourteen row boxes that nothing has asked to see.
    fn fill(&self, holder: &gtk4::Box, details: &Details) {
        let Some(panel) = drawer::panel(holder) else {
            return;
        };
        let (notice, rows) = match panel.child().and_downcast::<gtk4::Box>() {
            Some(page) => (
                page.first_child().and_downcast::<Notice>(),
                page.last_child().and_downcast::<gtk4::Box>(),
            ),
            None => {
                let page = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                page.add_css_class(DETAIL);
                let notice = Notice::new();
                notice.set_severity(Severity::Error);
                notice.set_visible(false);
                let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                page.append(&notice);
                page.append(&rows);
                panel.set_child(Some(&page));
                (Some(notice), Some(rows))
            }
        };

        if let Some(notice) = notice {
            notice.set_title(none_if_empty(&details.notice));
            notice.set_visible(!details.notice.is_empty());
        }

        let Some(rows) = rows else {
            return;
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
            imp.scan.upcast_ref::<gtk4::Widget>(),
            imp.more_paired.upcast_ref(),
            imp.more_nearby.upcast_ref(),
            imp.visible_as.upcast_ref(),
            imp.footer.upcast_ref(),
        ] {
            crate::set_css_class(widget, drawer::RECEDED, open.is_some());
        }
        crate::set_css_class(&*imp.hero, drawer::RECEDED, open.is_some());
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
        row.set_lead_icon(none_if_empty(&line.icon));
        crate::set_css_class(row, DANGER, line.destructive);

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
const DANGER: &str = "row--danger";

fn dress_row(row: &Row, entry: &Entry) {
    row.set_title(none_if_empty(&entry.title));
    row.set_subtitle(none_if_empty(&entry.subtitle));
    row.set_lead_icon(none_if_empty(&entry.icon));
    row.set_value(none_if_empty(&entry.value));
    row.set_selected(entry.selected);
}
