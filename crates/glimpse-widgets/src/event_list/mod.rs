mod imp;
mod row;

pub use row::EventRow;

use gettextrs::ngettext;
use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

use crate::reconcile::by_key;
use crate::{Expandable, Fact, FactList, Row, none_if_empty};

const MORE_ICON: &str = "view-more-symbolic";
const PAST: &str = "event-list__row--past";

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Event {
    pub id: String,
    pub summary: String,
    pub detail: String,
    pub when: String,
    pub color: Option<gdk::RGBA>,
    pub past: bool,
    pub links: Vec<Link>,
    pub facts: Vec<Fact>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Link {
    pub title: String,
    pub url: String,
}

glib::wrapper! {
    pub struct EventList(ObjectSubclass<imp::EventList>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for EventList {
    fn default() -> Self {
        Self::new()
    }
}

impl EventList {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_events(&self, events: &[Event]) {
        let imp = self.imp();
        if imp.events.borrow().as_slice() == events {
            return;
        }
        imp.events.replace(events.to_vec());
        self.render();
    }

    pub fn set_activatable(&self, activatable: bool) {
        if self.imp().activatable.replace(activatable) == activatable {
            return;
        }
        self.render();
    }

    pub fn overflows(&self) -> bool {
        self.imp().more.get_visible()
    }

    pub fn set_max_rows(&self, max: u32) {
        if self.imp().max_rows.replace(max) == max {
            return;
        }
        self.render();
    }

    pub fn fold(&self) {
        let imp = self.imp();
        let earlier = imp.show_earlier.replace(false);
        let all = imp.show_all.replace(false);
        if earlier || all {
            self.render();
        }
    }

    pub fn connect_activated<F: Fn(&Self, u32) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |list: Self, index: u32| f(&list, index)),
        )
    }

    pub fn connect_link_activated<F: Fn(&Self, String) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "link-activated",
            false,
            glib::closure_local!(move |list: Self, url: String| f(&list, url)),
        )
    }

    fn render(&self) {
        let imp = self.imp();
        let events = imp.events.borrow();
        let earlier = events.iter().take_while(|event| event.past).count();
        let start = match imp.show_earlier.get() {
            true => 0,
            false => earlier,
        };
        let end = match (imp.max_rows.get() as usize, imp.show_all.get()) {
            (0, _) | (_, true) => events.len(),
            (max, false) => events.len().min(earlier + max),
        };
        let shown = &events[start..end];
        imp.cards
            .borrow_mut()
            .retain(|id, _| shown.iter().any(|event| &event.id == id));
        let leads = shown.iter().any(|event| event.color.is_some());

        by_key(
            &imp.rows,
            &mut imp.holders.borrow_mut(),
            shown,
            |event| event.id.clone(),
            |event| self.build(event),
            |holder, event| self.apply(holder, event, leads),
        );

        count(&imp.earlier, start, "{count} earlier", "{count} earlier");
        count(
            &imp.more,
            events.len() - end,
            "{count} more event",
            "{count} more events",
        );
    }

    fn build(&self, event: &Event) -> Expandable {
        let row = EventRow::new();
        let id = event.id.clone();
        row.connect_clicked(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.activate(&id)
        ));
        Expandable::new(&row)
    }

    fn activate(&self, id: &str) {
        let imp = self.imp();
        if !imp.activatable.get() {
            return;
        }
        let index = imp.events.borrow().iter().position(|event| event.id == id);
        if let Some(index) = index {
            self.emit_by_name::<()>("activated", &[&(index as u32)]);
        }
    }

    fn apply(&self, holder: &Expandable, event: &Event, leads: bool) {
        let opens = !event.links.is_empty() || !event.facts.is_empty();
        if let Some(row) = holder.head::<EventRow>() {
            let item: &Row = row.upcast_ref();
            item.set_title(Some(event.summary.as_str()));
            item.set_subtitle(none_if_empty(&event.detail));
            item.set_activatable(opens || self.imp().activatable.get());
            crate::set_css_class(item, PAST, event.past);
            row.set_when(none_if_empty(&event.when));
            row.set_color(event.color, leads);
            row.set_opens(opens);
        }
        let mut cards = self.imp().cards.borrow_mut();
        let unchanged = cards
            .get(&event.id)
            .is_some_and(|held| held.links == event.links && held.facts == event.facts);
        if unchanged && holder.details::<gtk4::Widget>().is_some() == opens {
            return;
        }
        match opens {
            true => holder.set_details(Some(&self.card(event))),
            false => holder.set_details(None::<&gtk4::Widget>),
        }
        cards.insert(event.id.clone(), event.clone());
    }

    fn card(&self, event: &Event) -> gtk4::Box {
        let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        for link in &event.links {
            let row = Row::new();
            row.set_title(Some(link.title.as_str()));
            let url = link.url.clone();
            row.connect_clicked(glib::clone!(
                #[weak(rename_to = list)]
                self,
                move |_| list.emit_by_name::<()>("link-activated", &[&url])
            ));
            card.append(&row);
        }
        if !event.facts.is_empty() {
            let facts = FactList::new();
            facts.set_facts(&event.facts);
            card.append(&facts);
        }
        card
    }

    fn summary_at(&self, y: i32) -> Option<String> {
        let imp = self.imp();
        let holders = imp.holders.borrow();
        let (id, _) = holders.iter().find(|(_, holder)| {
            let Some(row) = holder.head::<gtk4::Widget>() else {
                return false;
            };
            let top = row
                .compute_bounds(self)
                .map(|bounds| bounds.y())
                .unwrap_or(0.0);
            let bottom = top + row.height() as f32;
            (top..bottom).contains(&(y as f32))
        })?;
        imp.events
            .borrow()
            .iter()
            .find(|event| &event.id == id)
            .map(|event| event.summary.clone())
    }
}

fn count(row: &Row, hidden: usize, singular: &str, plural: &str) {
    let visible = hidden > 0;
    if row.get_visible() != visible {
        row.set_visible(visible);
    }
    if visible {
        row.set_title(Some(
            ngettext(singular, plural, hidden as u32)
                .replace("{count}", &hidden.to_string())
                .as_str(),
        ));
    }
}

fn more_row() -> Row {
    let row = Row::new();
    row.set_lead_icon(Some(MORE_ICON));
    row.set_activatable(true);
    row.set_visible(false);
    row
}
