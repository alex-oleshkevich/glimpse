mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Row, SplitRow, SwitchRow, drawer, none_if_empty, reconcile, set_footer_row};

pub use imp::{Ask, Details, Entered, Entry, Line, Place, accepts};

const NETWORK: &str = "network-popover__row";
const DETAIL: &str = "detail-card";
const NETWORKS_PAGE: &str = "networks";
const PROMPT_PAGE: &str = "prompt";

glib::wrapper! {
    pub struct NetworkPopover(ObjectSubclass<imp::NetworkPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for NetworkPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_radio(&self, title: &str, subtitle: &str, icon: &str, on: bool, settable: bool) {
        let imp = self.imp();
        imp.hero.set_title(none_if_empty(title));
        imp.hero.set_subtitle(none_if_empty(subtitle));
        imp.hero.set_icon_name(none_if_empty(icon));

        imp.quiet.set(true);
        if imp.wifi.is_active() != on {
            imp.wifi.set_active(on);
        }
        imp.quiet.set(false);
        if imp.wifi.is_sensitive() != settable {
            imp.wifi.set_sensitive(settable);
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
        let empty = imp
            .entries
            .borrow()
            .iter()
            .all(|e| e.place != Place::Networks);
        let looking = scanning && empty;
        if imp.networks.empty() == looking {
            return;
        }
        imp.networks.set_empty(looking);
        imp.networks.set_visible(!empty || looking);
    }

    pub fn set_details(&self, details: Option<&Details>) {
        let imp = self.imp();
        if imp.details.borrow().as_ref() == details {
            return;
        }
        imp.details.replace(details.cloned());
        self.render_details();
    }

    pub fn set_overflow(&self, more: Option<&str>) {
        set_footer_row(&self.imp().more, more);
    }

    pub fn set_hidden_entry(&self, visible: bool) {
        let imp = self.imp();
        if imp.hidden.get_visible() != visible {
            imp.hidden.set_visible(visible);
        }
    }

    pub fn set_footer(&self, label: Option<&str>) {
        set_footer_row(&self.imp().footer, label);
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

    pub fn prompting(&self) -> bool {
        self.imp().prompt.borrow().is_some()
    }

    pub fn chosen(&self) -> u32 {
        self.imp().prompt_security.selected()
    }

    pub fn connect_answered<F: Fn(&Self, bool, &str) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "answered",
            false,
            glib::closure_local!(move |popover: Self, accepted: bool, entered: String| f(
                &popover, accepted, &entered
            )),
        )
    }

    #[cfg(test)]
    pub(crate) fn type_in(&self, text: &str) {
        let imp = self.imp();
        match imp.prompt_name.get_visible() {
            true => imp.prompt_name.set_text(text),
            false => imp.prompt_secret.set_text(text),
        }
    }

    #[cfg(test)]
    pub(crate) fn can_submit(&self) -> bool {
        self.imp().prompt_accept.is_sensitive()
    }

    pub(crate) fn answer(&self, accepted: bool) {
        let imp = self.imp();
        let entered = match accepted {
            true => imp.typed(),
            false => String::new(),
        };
        let kind = imp
            .prompt
            .borrow()
            .as_ref()
            .map(|ask| ask.entered)
            .unwrap_or_default();
        if accepted && !imp.open_chosen() && !imp::accepts(&entered, kind) {
            return;
        }
        self.emit_by_name::<()>("answered", &[&accepted, &entered]);
    }

    fn render_prompt(&self, switched: bool) {
        let imp = self.imp();
        if switched {
            imp.prompt_secret.set_text("");
            imp.prompt_name.set_text("");
        }

        let prompt = imp.prompt.borrow().clone();
        let Some(ask) = prompt else {
            imp.pages.set_visible_child_name(NETWORKS_PAGE);
            imp.hero.set_sensitive(true);
            imp.footer.set_sensitive(true);
            return;
        };

        crate::set_text(&imp.prompt_network, none_if_empty(&ask.network));
        crate::set_text(&imp.prompt_ask, none_if_empty(&ask.question));
        let naming = ask.entered == Entered::Name;
        imp.prompt_name.set_visible(naming);
        imp.prompt_security.set_visible(!ask.choices.is_empty());
        if switched && !ask.choices.is_empty() {
            let labels: Vec<&str> = ask.choices.iter().map(String::as_str).collect();
            imp.prompt_security
                .set_model(Some(&gtk4::StringList::new(&labels)));
            imp.prompt_security.set_selected(0);
        }
        if imp
            .prompt_accept
            .label()
            .is_none_or(|held| held != ask.accept)
        {
            imp.prompt_accept.set_label(&ask.accept);
        }
        imp.revalidate();

        imp.pages.set_visible_child_name(PROMPT_PAGE);
        imp.hero.set_sensitive(false);
        imp.footer.set_sensitive(false);
        match naming {
            true => imp.prompt_name.grab_focus(),
            false => imp.prompt_secret.grab_focus(),
        };
    }

    pub fn connect_wifi_toggled<F: Fn(&Self, bool) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "wifi-toggled",
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
            glib::closure_local!(move |popover: Self, id: String, active: bool| {
                f(&popover, &id, active)
            }),
        )
    }

    pub fn connect_selected<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "selected",
            false,
            glib::closure_local!(move |popover: Self, id: String| f(&popover, &id)),
        )
    }

    pub fn connect_acted<F: Fn(&Self, &str, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "acted",
            false,
            glib::closure_local!(move |popover: Self, id: String, action: String| {
                f(&popover, &id, &action)
            }),
        )
    }

    pub fn connect_toggled<F: Fn(&Self, &str, &str, bool) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "toggled",
            false,
            glib::closure_local!(move |popover: Self, id: String, action: String, on: bool| {
                f(&popover, &id, &action, on)
            }),
        )
    }

    pub fn connect_expanded<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "expanded",
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }

    pub fn connect_hidden_network<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "hidden-network",
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }

    pub fn connect_footer_activated<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "footer-activated",
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }

    fn active(&self, id: &str) -> bool {
        self.imp()
            .entries
            .borrow()
            .iter()
            .find(|entry| entry.id == id)
            .is_some_and(|entry| entry.selected)
    }

    fn render_entries(&self) {
        let imp = self.imp();
        let entries = imp.entries.borrow().clone();

        for (place, section, parent, held) in [
            (
                Place::Networks,
                &imp.networks,
                &imp.network_rows,
                &imp.network_held,
            ),
            (Place::Known, &imp.known, &imp.known_rows, &imp.known_held),
            (Place::Wired, &imp.wired, &imp.wired_rows, &imp.wired_held),
            (Place::Vpn, &imp.vpn, &imp.vpn_rows, &imp.vpn_held),
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
                |entry| drawer::holder(&self.build_row(&entry.id)),
                |holder, entry| {
                    if let Some(split) = drawer::head::<SplitRow>(holder) {
                        dress(&split, entry);
                    }
                },
            );
            section.set_visible(!wanted.is_empty() || section.empty());
        }
    }

    fn build_row(&self, id: &str) -> SplitRow {
        let split = SplitRow::new();
        split.add_css_class(NETWORK);

        let lock = gtk4::Image::from_icon_name("changes-prevent-symbolic");
        lock.set_accessible_role(gtk4::AccessibleRole::Presentation);
        lock.set_visible(false);
        split.set_trail(&lock);

        let key = id.to_owned();
        split.connect_activated(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| {
                let active = popover.active(&key);
                popover.emit_by_name::<()>("activated", &[&key, &active]);
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
            dress_line,
        );
    }

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
            imp.more.upcast_ref::<gtk4::Widget>(),
            imp.hidden.upcast_ref(),
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
            (&imp.networks, &imp.network_held),
            (&imp.known, &imp.known_held),
            (&imp.wired, &imp.wired_held),
            (&imp.vpn, &imp.vpn_held),
        ] {
            if section.get_visible() && !section.empty() {
                listed.extend(held.borrow().iter().cloned());
            }
        }
        listed
    }

    fn holders(&self) -> impl Iterator<Item = (String, gtk4::Box)> + use<> {
        let imp = self.imp();
        let mut all = imp.network_held.borrow().clone();
        for held in [&imp.known_held, &imp.wired_held, &imp.vpn_held] {
            all.extend(held.borrow().iter().cloned());
        }
        all.into_iter()
    }

    fn build_line(&self, id: &str, line: &Line) -> Row {
        let key = id.to_owned();
        let action = line.action.clone();

        if let Some(on) = line.toggle {
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
        row.connect_clicked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>("acted", &[&key, &action])
        ));
        row
    }
}

fn dress(split: &SplitRow, entry: &Entry) {
    let row = split.row();
    row.set_title(none_if_empty(&entry.title));
    row.set_subtitle(none_if_empty(&entry.subtitle));
    row.set_lead_icon(none_if_empty(&entry.icon));
    row.set_busy(entry.busy);
    row.set_selected(entry.selected);

    if let Some(lock) = row.trail() {
        lock.set_visible(entry.secured);
    }
}

fn dress_line(row: &Row, line: &Line) {
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
