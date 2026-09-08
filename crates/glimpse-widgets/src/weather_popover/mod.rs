mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Day, Fact, FactList, Hour, Notice, Section, Severity};

pub fn day_page(index: u32) -> String {
    format!("day{index}")
}

pub fn alert_page(index: usize) -> String {
    format!("alert{index}")
}

/// One `Notice`, wherever it came from. `page` is the drawer page it opens; `None` is a notice
/// that only states something, which is what decides whether it takes a click at all.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Advisory {
    pub severity: Severity,
    pub icon_name: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub page: Option<String>,
}

/// One page of the drawer. The details row and every day and alert build the same widget and
/// differ only in what they put in it.
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

        for (index, advisory) in alerts.iter().enumerate() {
            if notices.len() == index {
                notices.push(self.build_notice(index));
            }
            dress(&notices[index], advisory);
        }
        for notice in notices.split_off(alerts.len()) {
            imp.alerts.remove(&notice);
        }
        imp.alerts.set_visible(!alerts.is_empty());
    }

    pub fn set_pages(&self, pages: &[Page]) {
        let imp = self.imp();
        if imp.built.borrow().as_slice() == pages {
            return;
        }
        imp.built.replace(pages.to_vec());

        let open = imp.pages.visible_child_name().map(|name| name.to_string());
        while let Some(child) = imp.pages.first_child() {
            imp.pages.remove(&child);
        }
        for page in pages {
            imp.pages.add_named(&build(page), Some(page.key.as_str()));
        }

        match open.filter(|key| pages.iter().any(|page| &page.key == key)) {
            Some(key) => imp.pages.set_visible_child_name(&key),
            None => crate::drawer::set(&imp.drawer, false),
        }
    }

    /// A second activation of the page already showing closes the drawer, which is the only way
    /// back for whoever opened it.
    pub fn open(&self, key: &str) {
        let imp = self.imp();
        if imp.pages.child_by_name(key).is_none() {
            return;
        }
        if imp.pages.visible_child_name().as_deref() == Some(key) {
            return crate::drawer::toggle(&imp.drawer);
        }
        imp.pages.set_visible_child_name(key);
        crate::drawer::set(&imp.drawer, true);
    }

    pub fn is_open(&self) -> Option<String> {
        let imp = self.imp();
        imp.drawer
            .reveals_child()
            .then(|| imp.pages.visible_child_name().map(|name| name.to_string()))
            .flatten()
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
        self.imp().alerts.append(&notice);
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
    let section = Section::new();
    section.set_title(Some(page.title.as_str()));

    let body = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    if let Some(description) = &page.description {
        let label = gtk4::Label::new(Some(description.as_str()));
        label.set_wrap(true);
        label.set_xalign(0.0);
        label.add_css_class("drawer-page__description");
        body.append(&label);
    }

    let facts = FactList::new();
    facts.set_facts(&page.facts);
    body.append(&facts);

    section.set_content(Some(&body));
    section.upcast()
}
