mod imp;

use gettextrs::ngettext;
use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

use crate::reconcile::by_key;
use crate::{
    ActionRow, Dots, ExpanderRow, Fact, Row, RowTypography, clear_children, fact_row, none_if_empty,
};

const MORE_ICON: &str = "view-more-symbolic";
const DOT_SCALE: f32 = 3.0;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Event {
    pub id: String,
    pub summary: String,
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
            |event| (event.id.clone(), opens(event)),
            |event| match opens(event) {
                true => ExpanderRow::new().upcast(),
                false => Row::new().upcast(),
            },
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

    fn apply(&self, holder: &gtk4::Widget, event: &Event, leads: bool) {
        let expander = holder.downcast_ref::<ExpanderRow>();
        let row: Row = match expander {
            Some(expander) => expander.imp().head.get().upcast(),
            None => holder.clone().downcast().expect("an event row"),
        };
        row.set_title(Some(event.summary.as_str()));
        row.set_tooltip(Some(event.summary.as_str()));
        row.set_value(none_if_empty(&event.when));
        row.set_typography(RowTypography::Numeric);
        row.set_inactive(event.past);
        match leads {
            true => row.set_start(&dot(&row, event.color)),
            false => row.clear_start(),
        }

        let Some(expander) = expander else {
            self.imp().cards.borrow_mut().remove(&event.id);
            return;
        };
        let mut cards = self.imp().cards.borrow_mut();
        let unchanged = cards
            .get(&event.id)
            .is_some_and(|held| held.links == event.links && held.facts == event.facts);
        if unchanged {
            return;
        }
        let details = expander.details();
        clear_children(&details);
        for fact in &event.facts {
            details.append(&fact_row(fact));
        }

        for link in &event.links {
            let action = ActionRow::new();
            action
                .upcast_ref::<Row>()
                .set_title(Some(link.title.as_str()));
            let url = link.url.clone();
            action.connect_activated(glib::clone!(
                #[weak(rename_to = list)]
                self,
                move |_| list.emit_by_name::<()>("link-activated", &[&url])
            ));
            details.append(&action);
        }
        cards.insert(event.id.clone(), event.clone());
    }
}

fn opens(event: &Event) -> bool {
    !event.links.is_empty() || !event.facts.is_empty()
}

fn dot(row: &Row, color: Option<gdk::RGBA>) -> Dots {
    let dots = row
        .imp()
        .start
        .first_child()
        .and_downcast::<Dots>()
        .unwrap_or_else(|| {
            let dots = Dots::new();
            dots.set_max(1);
            dots.set_size(crate::dots::SIZE * DOT_SCALE);
            dots
        });
    dots.set_colors(color.as_slice());
    dots
}

fn count(row: &ActionRow, hidden: usize, singular: &str, plural: &str) {
    let visible = hidden > 0;
    if row.get_visible() != visible {
        row.set_visible(visible);
    }
    if visible {
        row.upcast_ref::<Row>().set_title(Some(
            ngettext(singular, plural, hidden as u32)
                .replace("{count}", &hidden.to_string())
                .as_str(),
        ));
    }
}

fn more_row() -> ActionRow {
    let row = ActionRow::new();
    row.upcast_ref::<Row>().set_icon_name(Some(MORE_ICON));
    row.set_visible(false);
    row
}
