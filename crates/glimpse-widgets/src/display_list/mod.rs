mod imp;

use gettextrs::gettext;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::reconcile::by_key;
use crate::{Expandable, Fact, FactList, Row, SwitchRow};

pub(crate) const ENABLE_REQUESTED: &str = "enable-requested";

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
        if imp.displays.borrow().as_slice() == displays {
            return;
        }
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

    fn render(&self) {
        let imp = self.imp();
        #[cfg(test)]
        imp.renders.set(imp.renders.get() + 1);
        let displays = imp.displays.borrow();
        let power = imp.output_power.get();
        let last_enabled = displays.iter().filter(|display| display.enabled).count() == 1;
        by_key(
            self,
            &mut imp.holders.borrow_mut(),
            &displays,
            |display| display.connector.clone(),
            |display| self.build(&display.connector),
            |holder, display| apply(holder, display, power, last_enabled),
        );
    }

    fn build(&self, connector: &str) -> Expandable {
        let row = Row::new();
        let chevron = gtk4::Image::from_icon_name("go-next-symbolic");
        chevron.set_accessible_role(gtk4::AccessibleRole::Presentation);
        chevron.add_css_class("drawer-chevron");
        row.set_trail(&chevron);

        let switch = SwitchRow::new();
        let head: &Row = switch.upcast_ref();
        head.set_title(Some(gettext("Enabled")));
        let connector = connector.to_owned();
        switch.connect_toggled(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_, on| list.emit_by_name::<()>(ENABLE_REQUESTED, &[&connector, &on])
        ));

        let body = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        body.append(&FactList::new());
        body.append(&switch);

        let holder = Expandable::new(&row);
        holder.set_details(Some(&body));
        holder
    }
}

fn apply(holder: &Expandable, display: &Display, power: bool, last_enabled: bool) {
    if let Some(row) = holder.head::<Row>() {
        row.set_title(Some(heading(display).as_str()));
    }
    let Some(body) = holder.details::<gtk4::Box>() else {
        return;
    };
    if let Some(facts) = body.first_child().and_downcast::<FactList>() {
        facts.set_facts(&facts_of(display));
    }
    let Some(switch) = body.last_child().and_downcast::<SwitchRow>() else {
        return;
    };
    switch.set_visible(power);
    if power {
        let locked = display.enabled && last_enabled;
        let head: &Row = switch.upcast_ref();
        head.set_subtitle(locked.then(|| gettext("The last enabled display can't be turned off")));
        switch.set_locked(locked);
        switch.set_active(display.enabled);
    }
}

fn heading(display: &Display) -> String {
    match display.label.is_empty() {
        true => display.connector.clone(),
        false => display.label.clone(),
    }
}

fn facts_of(display: &Display) -> Vec<Fact> {
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
