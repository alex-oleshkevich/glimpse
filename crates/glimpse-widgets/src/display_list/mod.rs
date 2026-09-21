mod imp;

use gettextrs::gettext;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Fact, FactList, Row, SwitchRow};

pub(crate) const ENABLE_REQUESTED: &str = "enable-requested";
pub(crate) const DETAILS_OPEN_CHANGED: &str = "details-open-changed";
const DETAIL: &str = "detail-card";

#[derive(Debug, Default, Clone, PartialEq)]
pub struct DisplayMode {
    pub width: i32,
    pub height: i32,
    pub refresh_mhz: i32,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct DisplayLogical {
    pub x: i32,
    pub y: i32,
    pub scale: f64,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Display {
    pub connector: String,
    pub label: String,
    pub make: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub current_mode: Option<DisplayMode>,
    pub logical: Option<DisplayLogical>,
    pub enabled: bool,
}

#[derive(Debug)]
pub struct Detail {
    facts: FactList,
    body: gtk4::Box,
    switch: SwitchRow,
}

glib::wrapper! {
    pub struct DisplayList(ObjectSubclass<imp::DisplayList>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for DisplayList {
    fn default() -> Self {
        Self::new()
    }
}

impl DisplayList {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_displays(&self, displays: &[Display]) {
        let imp = self.imp();
        imp.displays.replace(displays.to_vec());
        self.render();
    }

    pub fn set_output_power(&self, supported: bool) {
        let imp = self.imp();
        if imp.output_power.get() == supported {
            return;
        }
        imp.output_power.set(supported);
        self.render();
    }

    pub fn connect_enable_requested<F: Fn(&Self, String, bool) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            ENABLE_REQUESTED,
            false,
            glib::closure_local!(move |list: Self, connector: String, enabled: bool| {
                f(&list, connector, enabled)
            }),
        )
    }

    pub fn connect_details_open_changed<F: Fn(&Self, bool) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            DETAILS_OPEN_CHANGED,
            false,
            glib::closure_local!(move |list: Self, open: bool| f(&list, open)),
        )
    }

    fn render(&self) {
        let imp = self.imp();
        #[cfg(test)]
        imp.renders.set(imp.renders.get() + 1);
        {
            let displays = imp.displays.borrow();
            let power = imp.output_power.get();
            let last_enabled = displays.iter().filter(|display| display.enabled).count() == 1;
            let mut rows = imp.rows.borrow_mut();
            let mut holders = imp.holders.borrow_mut();
            let mut details = imp.details.borrow_mut();

            for (index, display) in displays.iter().enumerate() {
                if rows.len() == index {
                    let (row, holder, detail) = self.build_row(index as u32);
                    holder.insert_after(self, holders.last());
                    holders.push(holder);
                    rows.push(row);
                    details.push(detail);
                }
                let row = &rows[index];
                row.set_title(Some(heading(display).as_str()));

                let detail = &details[index];
                detail.facts.set_facts(&facts(display));

                match power {
                    true => {
                        if detail.switch.parent().is_none() {
                            detail.body.append(&detail.switch);
                        }
                        let locked = display.enabled && last_enabled;
                        let head: &Row = detail.switch.upcast_ref();
                        head.set_subtitle(
                            locked.then(|| gettext("The last enabled display can't be turned off")),
                        );
                        detail.switch.set_locked(locked);
                        detail.switch.set_active(display.enabled);
                    }
                    false => {
                        if detail.switch.parent().is_some() {
                            detail.body.remove(&detail.switch);
                        }
                    }
                }
            }

            rows.truncate(displays.len());
            details.truncate(displays.len());
            for holder in holders.split_off(displays.len()) {
                holder.unparent();
            }
        }

        self.apply_reveal();
    }

    fn build_row(&self, index: u32) -> (Row, gtk4::Box, Detail) {
        let row = Row::new();
        let chevron = gtk4::Image::from_icon_name("go-next-symbolic");
        chevron.set_accessible_role(gtk4::AccessibleRole::Presentation);
        chevron.add_css_class("drawer-chevron");
        row.set_trail(&chevron);
        row.connect_clicked(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.toggle(index)
        ));

        let facts = FactList::new();

        let switch = SwitchRow::new();
        let head: &Row = switch.upcast_ref();
        head.set_title(Some(gettext("Enabled")));
        switch.connect_toggled(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_, on| list.report(index, on)
        ));

        let body = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        body.add_css_class(DETAIL);
        body.append(&facts);

        let holder = crate::drawer::holder(&row);
        if let Some(panel) = crate::drawer::panel(&holder) {
            panel.set_child(Some(&body));
        }

        (
            row,
            holder,
            Detail {
                facts,
                body,
                switch,
            },
        )
    }

    fn toggle(&self, index: u32) {
        let imp = self.imp();
        {
            let holders = imp.holders.borrow();
            let target = index as usize;
            let opening = holders
                .get(target)
                .and_then(crate::drawer::panel)
                .is_some_and(|panel| !panel.reveals_child());

            for (at, holder) in holders.iter().enumerate() {
                if let Some(panel) = crate::drawer::panel(holder) {
                    crate::drawer::set(&panel, at == target && opening);
                }
            }
        }
        self.apply_reveal();
    }

    fn apply_reveal(&self) {
        let any_open = {
            let holders = self.imp().holders.borrow();
            let any_open = holders.iter().any(|holder| {
                crate::drawer::panel(holder).is_some_and(|panel| panel.reveals_child())
            });

            for holder in holders.iter() {
                let Some(panel) = crate::drawer::panel(holder) else {
                    continue;
                };
                let open = panel.reveals_child();
                if let Some(row) = crate::drawer::head::<Row>(holder) {
                    crate::set_css_class(&row, crate::drawer::OPEN, open);
                    crate::set_css_class(&row, crate::drawer::RECEDED, any_open && !open);
                }
            }
            any_open
        };

        if self.imp().details_open.replace(any_open) != any_open {
            self.emit_by_name::<()>(DETAILS_OPEN_CHANGED, &[&any_open]);
        }
    }

    fn report(&self, index: u32, enabled: bool) {
        let connector = self
            .imp()
            .displays
            .borrow()
            .get(index as usize)
            .map(|display| display.connector.clone());

        if let Some(connector) = connector {
            self.emit_by_name::<()>(ENABLE_REQUESTED, &[&connector, &enabled]);
        }
    }
}

fn heading(display: &Display) -> String {
    match display.label.is_empty() {
        true => display.connector.clone(),
        false => display.label.clone(),
    }
}

fn facts(display: &Display) -> Vec<Fact> {
    let mut facts = vec![Fact::new(gettext("Connector"), &display.connector)];

    if let Some(make) = non_empty(display.make.as_deref()) {
        facts.push(Fact::new(gettext("Make"), make));
    }
    if let Some(model) = non_empty(display.model.as_deref()) {
        facts.push(Fact::new(gettext("Model"), model));
    }
    if let Some(serial) = non_empty(display.serial.as_deref()) {
        facts.push(Fact::new(gettext("Serial"), serial));
    }
    if let Some(mode) = &display.current_mode {
        facts.push(Fact::new(gettext("Current mode"), mode_summary(mode)));
    }
    if let Some(logical) = &display.logical {
        facts.push(Fact::new(gettext("Scale"), scale_summary(logical.scale)));
        facts.push(Fact::new(gettext("Position"), position_summary(logical)));
    }

    facts
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value.filter(|value| !value.is_empty()).map(str::to_owned)
}

fn mode_summary(mode: &DisplayMode) -> String {
    gettext("{width} × {height} · {hz} Hz")
        .replace("{width}", &mode.width.to_string())
        .replace("{height}", &mode.height.to_string())
        .replace("{hz}", &refresh_hz(mode.refresh_mhz))
}

fn refresh_hz(refresh_mhz: i32) -> String {
    match refresh_mhz % 1000 {
        0 => (refresh_mhz / 1000).to_string(),
        _ => format!("{:.2}", refresh_mhz as f64 / 1000.0),
    }
}

fn scale_summary(scale: f64) -> String {
    crate::percent_text(scale * 100.0)
}

fn position_summary(logical: &DisplayLogical) -> String {
    gettext("{x}, {y}")
        .replace("{x}", &logical.x.to_string())
        .replace("{y}", &logical.y.to_string())
}
