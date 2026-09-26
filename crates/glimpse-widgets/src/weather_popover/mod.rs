mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Day, Expandable, Fact, FactList, Hour, Notice, Section, Severity};

const DESCRIPTION: &str = "detail-card__description";

pub fn day_page(index: u32) -> String {
    format!("day{index}")
}

pub fn alert_page(index: usize) -> String {
    format!("alert{index}")
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Advisory {
    pub severity: Severity,
    pub icon_name: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub page: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Page {
    pub key: String,
    pub title: String,
    pub description: Option<String>,
    pub facts: Vec<Fact>,
}

glib::wrapper! {
    pub struct WeatherPopover(ObjectSubclass<imp::WeatherPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for WeatherPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl WeatherPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_heading(&self, icon_name: &str, title: &str, subtitle: Option<&str>) {
        let imp = self.imp();
        imp.hero.set_icon_name(Some(icon_name));
        imp.hero.set_title(Some(title));
        imp.hero.set_subtitle(subtitle);
    }

    pub fn set_reading(&self, reading: Option<(&str, &str)>) {
        let readout = &self.imp().reading;
        readout.set_value(reading.map(|(value, _)| value));
        readout.set_unit(reading.map(|(_, unit)| unit));
        readout.set_visible(reading.is_some());
    }

    pub fn set_unit(&self, unit: &str) {
        let imp = self.imp();
        imp.hours.set_unit(unit);
        imp.days.set_unit(unit);
    }

    pub fn set_hours(&self, hours: &[Hour]) {
        let imp = self.imp();
        imp.hours.set_hours(hours);
        show(&imp.hourly, &imp.hourly_rule, !hours.is_empty());
    }

    pub fn set_days(&self, days: &[Day]) {
        let imp = self.imp();
        imp.days.set_days(days);
        show(&imp.daily, &imp.daily_rule, !days.is_empty());
    }

    pub fn set_nowcast(&self, nowcast: Option<&Advisory>) {
        let notice = &self.imp().nowcast;
        match nowcast {
            Some(advisory) => dress(notice, advisory),
            None => notice.set_visible(false),
        }
    }

    pub fn set_alerts(&self, alerts: &[Advisory]) {
        let imp = self.imp();
        let mut holders = imp.notices.borrow_mut();
        let before = holders.len();
        for (index, advisory) in alerts.iter().enumerate() {
            if holders.len() == index {
                let holder = Expandable::new(&Notice::new());
                imp.alerts.append(&holder);
                holders.push(holder);
            }
            if let Some(notice) = holders[index].head::<Notice>() {
                dress(&notice, advisory);
            }
        }
        for holder in holders.split_off(alerts.len()) {
            imp.alerts.remove(&holder);
        }
        imp.alerts.set_visible(!alerts.is_empty());

        let grew = holders.len() > before;
        drop(holders);
        if grew {
            self.fill_alerts();
        }
    }

    pub fn set_pages(&self, pages: &[Page]) {
        let imp = self.imp();
        if imp.built.borrow().as_slice() == pages {
            return;
        }
        imp.built.replace(pages.to_vec());

        let mut days: Vec<Option<gtk4::Widget>> = Vec::new();
        for page in pages {
            let Some(Slot::Day(index)) = locate(&page.key) else {
                continue;
            };
            if days.len() <= index {
                days.resize(index + 1, None);
            }
            days[index] = Some(build(page));
        }
        imp.days.set_details(&days);
        self.fill_alerts();
    }

    fn fill_alerts(&self) {
        let imp = self.imp();
        let pages = imp.built.borrow();
        for (index, holder) in imp.notices.borrow().iter().enumerate() {
            let page = pages
                .iter()
                .find(|page| matches!(locate(&page.key), Some(Slot::Alert(at)) if at == index));
            holder.set_details(page.map(build).as_ref());
        }
    }

    pub fn set_footer(&self, label: Option<&str>) {
        crate::set_footer_row(&self.imp().footer, label);
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
}

fn dress(notice: &Notice, advisory: &Advisory) {
    notice.set_icon_name(Some(advisory.icon_name.as_str()));
    notice.set_title(Some(advisory.title.as_str()));
    notice.set_subtitle(advisory.subtitle.as_deref());
    notice.set_severity(advisory.severity);
    notice.set_activatable(advisory.page.is_some());
    notice.set_visible(true);
}

fn show(section: &Section, rule: &gtk4::Separator, visible: bool) {
    section.set_visible(visible);
    rule.set_visible(visible);
}

fn build(page: &Page) -> gtk4::Widget {
    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    card.update_property(&[gtk4::accessible::Property::Label(page.title.as_str())]);

    if let Some(description) = &page.description {
        let label = gtk4::Label::new(Some(description.as_str()));
        label.set_wrap(true);
        label.set_xalign(0.0);
        label.add_css_class(DESCRIPTION);
        card.append(&label);
    }

    let facts = FactList::new();
    facts.set_facts(&page.facts);
    card.append(&facts);
    card.upcast()
}

#[derive(Clone, Copy)]
enum Slot {
    Day(usize),
    Alert(usize),
}

fn locate(key: &str) -> Option<Slot> {
    if let Some(index) = key.strip_prefix("day") {
        return index.parse().ok().map(Slot::Day);
    }
    key.strip_prefix("alert")
        .and_then(|index| index.parse().ok())
        .map(Slot::Alert)
}
