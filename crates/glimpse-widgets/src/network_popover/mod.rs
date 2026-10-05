mod imp;
mod row;

use gettextrs::gettext;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Row, RowTone, none_if_empty, reconcile};
use row::NetworkRow;

pub use imp::{Ask, Details, Entered, Entry, Line, Place, accepts};

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
        imp.shell.set_title(none_if_empty(title));
        imp.shell.set_subtitle(none_if_empty(subtitle));
        imp.shell.set_icon_name(none_if_empty(icon));
        imp.radio_settable.set(settable);

        imp.quiet.set(true);
        if imp.wifi.is_active() != on {
            imp.wifi.set_active(on);
        }
        imp.quiet.set(false);
        let sensitive = settable && !self.prompting();
        if imp.wifi.is_sensitive() != sensitive {
            imp.wifi.set_sensitive(sensitive);
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

    pub fn set_scanning(&self, scanning: bool) {
        let imp = self.imp();
        let empty = imp
            .entries
            .borrow()
            .iter()
            .all(|e| !matches!(e.place, Place::Other | Place::Wifi));
        let looking = scanning && empty && !imp.more.get_visible();
        if imp.other.empty() != looking {
            imp.other.set_empty(looking);
        }
        self.settle_other();
    }

    pub fn set_details(&self, details: &[Details]) {
        let imp = self.imp();
        if imp.details.borrow().as_slice() == details {
            return;
        }
        imp.details.replace(details.to_vec());
        for (id, row) in self.holders() {
            self.fill(&id, &row);
        }
    }

    pub fn set_others(&self, count: usize, open: bool, rest: Option<&str>) {
        let imp = self.imp();
        let header = &imp.more;
        if header.get_visible() != (count > 0) {
            header.set_visible(count > 0);
        }
        imp.quiet.set(true);
        header.set_open(count > 0 && open && !self.prompting());
        imp.quiet.set(false);
        imp.all.upcast_ref::<Row>().set_title(rest);
        if imp.all.get_visible() != rest.is_some() {
            imp.all.set_visible(rest.is_some());
        }
        self.settle_other();
    }

    pub fn set_hidden_entry(&self, visible: bool) {
        let imp = self.imp();
        if imp.hidden.get_visible() != visible {
            imp.hidden.set_visible(visible);
            imp.hidden_rule.set_visible(visible);
        }
    }

    /// Other networks shows while it has a header or a scan to wait on. Collapsed, it is only the
    /// header, which is why its visibility follows the header and not the entries.
    fn settle_other(&self) {
        let imp = self.imp();
        let rows = imp
            .entries
            .borrow()
            .iter()
            .any(|entry| entry.place == Place::Other);
        let visible = rows || imp.more.get_visible() || imp.other.empty();
        if imp.other.get_visible() != visible {
            imp.other.set_visible(visible);
        }
    }

    pub fn set_footer(&self, label: Option<&str>) {
        let footer = &self.imp().footer;
        footer.upcast_ref::<Row>().set_title(label);
        if footer.get_visible() != label.is_some() {
            footer.set_visible(label.is_some());
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
            imp.wifi.set_sensitive(imp.radio_settable.get());
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

        for (_, row) in self.holders() {
            row.imp().expander.set_open(false);
        }
        imp.quiet.set(true);
        imp.more.set_open(false);
        imp.quiet.set(false);
        imp.pages.set_visible_child_name(PROMPT_PAGE);
        imp.wifi.set_sensitive(false);
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

    pub fn connect_activated<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
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

    pub fn connect_show_all<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "show-all",
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

    fn render_entries(&self) {
        let imp = self.imp();
        let entries = imp.entries.borrow().clone();

        for (place, section, parent, held) in [
            (Place::Other, &imp.other, &imp.other_rows, &imp.other_held),
            (
                Place::Wifi,
                &imp.wifi_section,
                &imp.wifi_rows,
                &imp.wifi_held,
            ),
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
                |entry| self.build_row(entry),
                |row, entry| self.apply(row, entry),
            );
            if place != Place::Other {
                section.set_visible(!wanted.is_empty());
            }
        }
        self.settle_other();
    }

    fn build_row(&self, entry: &Entry) -> NetworkRow {
        let row: NetworkRow = glib::Object::new();
        let key = entry.id.clone();
        row.imp().connection.connect_activated(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| {
                let entry = popover
                    .imp()
                    .entries
                    .borrow()
                    .iter()
                    .find(|entry| entry.id == key)
                    .cloned();
                let Some(entry) = entry else {
                    return;
                };
                if entry.selected {
                    let action = match entry.place {
                        Place::Vpn => "disconnect-vpn",
                        _ => "disconnect",
                    };
                    popover.emit_by_name::<()>("acted", &[&key, &action]);
                } else {
                    popover.emit_by_name::<()>("activated", &[&key]);
                }
            }
        ));
        let key = entry.id.clone();
        row.imp().autoconnect.connect_toggled(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_, on| popover.emit_by_name::<()>("toggled", &[&key, &"autoconnect", &on])
        ));
        let key = entry.id.clone();
        row.imp().forget.connect_activated(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>("acted", &[&key, &"forget"])
        ));
        row
    }

    fn apply(&self, row: &NetworkRow, entry: &Entry) {
        let imp = row.imp();
        imp.expander.set_title(none_if_empty(&entry.title));
        imp.expander.set_icon_name(none_if_empty(&entry.icon));
        imp.expander.set_busy(entry.busy);
        imp.status
            .set_css_classes(if entry.selected { &["accent"] } else { &[] });
        let status = match entry.selected {
            true => "object-select-symbolic",
            false => "changes-prevent-symbolic",
        };
        if imp.status.icon_name().as_deref() != Some(status) {
            imp.status.set_icon_name(Some(status));
        }
        let visible = entry.secured || entry.selected;
        if imp.status.get_visible() != visible {
            imp.status.set_visible(visible);
        }
        let connection = imp.connection.upcast_ref::<Row>();
        connection.set_title(Some(match entry.selected {
            true => gettext("Disconnect"),
            false => gettext("Connect"),
        }));
        connection.set_busy(entry.busy);
        self.fill(&entry.id, row);
    }

    fn fill(&self, id: &str, row: &NetworkRow) {
        let imp = self.imp();
        let details = imp
            .details
            .borrow()
            .iter()
            .find(|details| details.id == id)
            .cloned();
        let card = row.imp();
        if details.is_none() {
            card.expander.set_open(false);
        }
        let details = details.unwrap_or_default();
        card.facts.set_facts(&details.facts);
        if card.facts.get_visible() == details.facts.is_empty() {
            card.facts.set_visible(!details.facts.is_empty());
        }
        let primary = details.lines.iter().find(|line| {
            matches!(
                line.action.as_str(),
                "connect" | "disconnect" | "disconnect-vpn"
            )
        });
        let connection = card.connection.upcast_ref::<Row>();
        connection.set_tone(match primary.is_some_and(|line| line.destructive) {
            true => RowTone::Destructive,
            false => RowTone::Normal,
        });
        connection.set_busy(primary.map_or_else(|| card.expander.busy(), |line| line.busy));
        let autoconnect = details
            .lines
            .iter()
            .find(|line| line.action == "autoconnect");
        let on = autoconnect.and_then(|line| line.toggle);
        card.autoconnect.set_active(on.unwrap_or(false));
        card.autoconnect
            .upcast_ref::<Row>()
            .set_busy(autoconnect.is_some_and(|line| line.busy));
        if card.autoconnect.get_visible() != on.is_some() {
            card.autoconnect.set_visible(on.is_some());
        }
        let forget = details.lines.iter().find(|line| line.action == "forget");
        card.forget
            .upcast_ref::<Row>()
            .set_busy(forget.is_some_and(|line| line.busy));
        card.forget
            .upcast_ref::<Row>()
            .set_inactive(forget.is_none_or(|line| !line.activates));
        if card.forget.get_visible() != forget.is_some() {
            card.forget.set_visible(forget.is_some());
        }
    }

    fn holders(&self) -> impl Iterator<Item = (String, NetworkRow)> + use<> {
        let imp = self.imp();
        let mut all = imp.other_held.borrow().clone();
        for held in [&imp.wifi_held, &imp.wired_held, &imp.vpn_held] {
            all.extend(held.borrow().iter().cloned());
        }
        all.into_iter()
    }
}
