mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Expandable, Row, SplitRow, SwitchRow, none_if_empty, reconcile, set_footer_row};

pub use imp::{Ask, Details, Entered, Entry, Line, Place, accepts};

const NETWORK: &str = "network-popover__row";
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

    /// Every listed entry's detail, keyed by `Details::id`. A card is built the first time its
    /// chevron opens it and refreshed from here afterwards; an entry whose detail is gone closes.
    pub fn set_details(&self, details: &[Details]) {
        let imp = self.imp();
        if imp.details.borrow().as_slice() == details {
            return;
        }
        imp.details.replace(details.to_vec());
        for (id, holder) in self.holders() {
            if let Some(split) = holder.head::<SplitRow>() {
                split.set_detail_visible(self.carded(&id));
            }
            if holder.details::<gtk4::Widget>().is_some() {
                self.fill(&id, &holder);
            }
        }
    }

    /// Other networks as a disclosure: the header turns its chevron while `open`, and `rest` is the row at the bottom of an open list that the cap has
    /// cut short. No other network in range hides the header.
    pub fn set_others(&self, count: usize, open: bool, rest: Option<&str>) {
        let imp = self.imp();
        let header = &imp.more;
        if header.get_visible() != (count > 0) {
            header.set_visible(count > 0);
        }
        crate::set_css_class(&**header, crate::drawer::OPEN, open);
        crate::drawer::set(&imp.others_drawer, open);
        set_footer_row(&imp.all, rest);
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

        for (_, holder) in self.holders() {
            holder.set_expanded(false);
        }
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

    /// A row that is not in use was pressed: join it. A row in use opens its card instead, and
    /// never reports here.
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

    fn carded(&self, id: &str) -> bool {
        self.imp()
            .details
            .borrow()
            .iter()
            .any(|details| details.id == id)
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
                |entry| Expandable::new(&self.head_for(entry)),
                |holder, entry| self.apply(holder, entry),
            );
            if place != Place::Other {
                section.set_visible(!wanted.is_empty());
            }
        }
        self.settle_other();
        let live: Vec<String> = entries.iter().map(|entry| entry.id.clone()).collect();
        imp.lines.borrow_mut().retain(|id, _| live.contains(id));
    }

    /// A row in use is about that connection: the whole row opens its card, where Disconnect
    /// lives. Every other row joins on its body, and its chevron shows only when it has a card.
    fn head_for(&self, entry: &Entry) -> gtk4::Widget {
        if entry.selected {
            let row = Row::new();
            row.add_css_class(NETWORK);
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
        split.add_css_class(NETWORK);

        let lock = gtk4::Image::from_icon_name("changes-prevent-symbolic");
        lock.set_accessible_role(gtk4::AccessibleRole::Presentation);
        lock.set_visible(false);
        split.set_trail(&lock);

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
        if entry.selected == holder.head::<SplitRow>().is_some() {
            holder.set_head(&self.head_for(entry));
        }
        match holder.head::<SplitRow>() {
            Some(split) => {
                let row = split.row();
                dress(&row, entry);
                if let Some(lock) = row.trail() {
                    lock.set_visible(entry.secured);
                }
                split.set_detail_visible(self.carded(&entry.id));
            }
            None => {
                if let Some(row) = holder.head::<Row>() {
                    dress(&row, entry);
                }
            }
        }
    }

    fn open(&self, id: &str) {
        let holder = self.holders().find(|(held, _)| held == id);
        if let Some((_, holder)) = holder
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
            dress_line,
        );
    }

    fn holders(&self) -> impl Iterator<Item = (String, Expandable)> + use<> {
        let imp = self.imp();
        let mut all = imp.other_held.borrow().clone();
        for held in [&imp.wifi_held, &imp.wired_held, &imp.vpn_held] {
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

fn dress(row: &Row, entry: &Entry) {
    row.set_title(none_if_empty(&entry.title));
    row.set_subtitle(none_if_empty(&entry.subtitle));
    row.set_lead_icon(none_if_empty(&entry.icon));
    row.set_busy(entry.busy);
    row.set_selected(entry.selected);
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
