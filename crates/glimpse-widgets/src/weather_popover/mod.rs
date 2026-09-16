mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Day, Fact, FactList, Hour, Notice, Section, Severity, drawer};

const DETAIL: &str = "detail-card";
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
        let mut notices = imp.notices.borrow_mut();

        imp.keys.replace(
            alerts
                .iter()
                .map(|advisory| advisory.page.clone())
                .collect(),
        );

        let before = notices.len();
        for (index, advisory) in alerts.iter().enumerate() {
            if notices.len() == index {
                notices.push(self.build_notice(index));
            }
            dress(&notices[index], advisory);
        }
        for notice in notices.split_off(alerts.len()) {
            if let Some(holder) = notice.parent().and_downcast::<gtk4::Box>() {
                imp.alerts.remove(&holder);
            }
        }
        imp.alerts.set_visible(!alerts.is_empty());

        let grew = notices.len() > before;
        drop(notices);
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

        if !self
            .is_open()
            .is_some_and(|key| pages.iter().any(|page| page.key == key))
        {
            self.reveal(None);
        }
    }

    /// A second activation of the detail already showing closes it, which is the only way back for
    /// whoever opened it.
    pub fn open(&self, key: &str) {
        let Some(slot) = locate(key) else {
            return;
        };
        match self.is_open().as_deref() == Some(key) {
            true => self.reveal(None),
            false => self.reveal(Some(slot)),
        }
    }

    pub fn is_open(&self) -> Option<String> {
        let imp = self.imp();
        if let Some(index) = imp.days.revealed() {
            return Some(day_page(index as u32));
        }
        imp.notices
            .borrow()
            .iter()
            .position(|notice| panel_of(notice).is_some_and(|panel| panel.reveals_child()))
            .map(alert_page)
    }

    fn fill_alerts(&self) {
        let imp = self.imp();
        let pages = imp.built.borrow();
        for (index, notice) in imp.notices.borrow().iter().enumerate() {
            let Some(panel) = panel_of(notice) else {
                continue;
            };
            let page = pages
                .iter()
                .find(|page| matches!(locate(&page.key), Some(Slot::Alert(at)) if at == index));
            panel.set_child(page.map(build).as_ref());
        }
    }

    fn reveal(&self, slot: Option<Slot>) {
        let imp = self.imp();
        let day = match slot {
            Some(Slot::Day(index)) => Some(index),
            _ => None,
        };
        imp.days.reveal(day);
        if day.is_none() {
            imp.days.recede(slot.is_some());
        }

        for (index, notice) in imp.notices.borrow().iter().enumerate() {
            let Some(panel) = panel_of(notice) else {
                continue;
            };
            let open =
                matches!(slot, Some(Slot::Alert(at)) if at == index) && panel.child().is_some();
            drawer::set(&panel, open);
            crate::set_css_class(notice, drawer::OPEN, open);
            crate::set_css_class(notice, drawer::RECEDED, slot.is_some() && !open);
        }

        self.recede(slot.is_some());
    }

    fn recede(&self, any: bool) {
        let imp = self.imp();
        for widget in [
            imp.hero.upcast_ref::<gtk4::Widget>(),
            imp.hourly.upcast_ref(),
            imp.nowcast.upcast_ref(),
            imp.footer.upcast_ref(),
        ] {
            crate::set_css_class(widget, drawer::RECEDED, any);
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

    /// The handler is connected once, when the notice is built, and reads the key back by
    /// position. Connecting it while dressing would stack one handler per reconcile.
    fn build_notice(&self, index: usize) -> Notice {
        let notice = Notice::new();
        notice.connect_clicked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| {
                let key = popover.imp().keys.borrow().get(index).cloned().flatten();
                if let Some(key) = key {
                    popover.open(&key);
                }
            }
        ));
        self.imp().alerts.append(&drawer::holder(&notice));
        notice
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
    card.add_css_class(DETAIL);
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

fn panel_of(notice: &Notice) -> Option<gtk4::Revealer> {
    notice
        .parent()
        .and_downcast::<gtk4::Box>()
        .as_ref()
        .and_then(drawer::panel)
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
