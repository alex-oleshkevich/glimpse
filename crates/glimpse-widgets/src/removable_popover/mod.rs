mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{
    Expandable, FactList, Row, SplitRow, none_if_empty, reconcile, set_css_class, set_footer_row,
};

pub use imp::{Drive, Volume};

const CHEVRON: &str = "go-next-symbolic";
const DIMMED: &str = "dimmed";
const WARNING: &str = "row--warning";
const CAPACITY_BAR: &str = "capacity-bar";

#[derive(Debug, Clone, PartialEq)]
enum Release {
    Eject(String),
    Unmount(String),
}

#[derive(Debug, Default, Clone, PartialEq)]
struct Card {
    bar: bool,
    open: Option<String>,
    release: Option<Release>,
    facts: Vec<crate::Fact>,
}

impl Card {
    fn is_empty(&self) -> bool {
        !self.bar && self.open.is_none() && self.release.is_none() && self.facts.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Head {
    Opens,
    Split,
    Acts,
    Inert,
}

glib::wrapper! {
    pub struct RemovablePopover(ObjectSubclass<imp::RemovablePopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for RemovablePopover {
    fn default() -> Self {
        Self::new()
    }
}

struct DeviceRow {
    id: String,
    title: String,
    subtitle: String,
    icon: String,
    head: Head,
    fraction: Option<f64>,
    busy: bool,
    dimmed: bool,
    warning: bool,
    indent: bool,
    card: Card,
}

impl RemovablePopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_devices(&self, drives: &[Drive]) {
        let imp = self.imp();
        if imp.devices_data.borrow().as_slice() == drives {
            return;
        }
        imp.devices_data.replace(drives.to_vec());
        self.render_devices();
    }

    pub fn set_overflow(&self, devices: Option<&str>) {
        set_footer_row(&self.imp().devices_more, devices);
    }

    pub fn set_footer(&self, label: Option<&str>) {
        set_footer_row(&self.imp().footer, label);
    }

    pub fn connect_activated<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |popover: Self, id: String| f(&popover, &id)),
        )
    }

    pub fn connect_more<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "more",
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }

    pub fn connect_eject<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "eject",
            false,
            glib::closure_local!(move |popover: Self, id: String| f(&popover, &id)),
        )
    }

    pub fn connect_unmount<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "unmount",
            false,
            glib::closure_local!(move |popover: Self, id: String| f(&popover, &id)),
        )
    }

    pub fn connect_footer_activated<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "footer-activated",
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }

    fn render_devices(&self) {
        let imp = self.imp();
        let drives = imp.devices_data.borrow().clone();
        let rows = flatten_devices(&drives);
        imp.devices.set_visible(!rows.is_empty());
        imp.cards
            .borrow_mut()
            .retain(|id, _| rows.iter().any(|row| &row.id == id));
        reconcile::by_key(
            &*imp.devices_rows,
            &mut imp.devices_held.borrow_mut(),
            &rows,
            |row| row.id.clone(),
            |_| Expandable::default(),
            |holder, spec| self.apply_device(holder, spec),
        );
    }

    fn apply_device(&self, holder: &Expandable, spec: &DeviceRow) {
        let row = self.ensure_head(holder, spec);
        row.set_title(none_if_empty(&spec.title));
        row.set_subtitle(none_if_empty(&spec.subtitle));
        row.set_lead_icon(none_if_empty(&spec.icon));
        row.set_busy(spec.busy);
        set_css_class(&row, DIMMED, spec.dimmed);
        set_css_class(&row, WARNING, spec.warning);

        let margin = if spec.indent { 24 } else { 0 };
        if holder.margin_start() != margin {
            holder.set_margin_start(margin);
        }

        let mut cards = self.imp().cards.borrow_mut();
        if cards.get(&spec.id) != Some(&spec.card) {
            match spec.card.is_empty() {
                true => holder.set_details(None::<&gtk4::Widget>),
                false => holder.set_details(Some(&self.card(&spec.card))),
            }
            cards.insert(spec.id.clone(), spec.card.clone());
        }
        if let Some(bar) = holder
            .details::<gtk4::Box>()
            .and_then(|card| card.first_child())
            .and_downcast::<gtk4::ProgressBar>()
            && let Some(fraction) = spec.fraction
            && bar.fraction() != fraction
        {
            bar.set_fraction(fraction);
        }
    }

    fn ensure_head(&self, holder: &Expandable, spec: &DeviceRow) -> Row {
        let split = spec.head == Head::Split;
        if let Some(existing) = holder.head::<SplitRow>().filter(|_| split) {
            return existing.row();
        }
        if let Some(existing) = holder.head::<Row>().filter(|_| !split) {
            set_opens(&existing, spec.head);
            return existing;
        }
        holder.set_expanded(false);
        let key = spec.id.clone();
        match split {
            true => {
                let split_row = SplitRow::new();
                split_row.set_detail_tooltip(Some(gettextrs::gettext("Details")));
                split_row.connect_activated(glib::clone!(
                    #[weak(rename_to = popover)]
                    self,
                    move |_| popover.emit_by_name::<()>("activated", &[&key])
                ));
                holder.set_head(&split_row);
                split_row.row()
            }
            false => {
                let row = Row::new();
                let chevron = gtk4::Image::from_icon_name(CHEVRON);
                chevron.set_accessible_role(gtk4::AccessibleRole::Presentation);
                chevron.add_css_class("drawer-chevron");
                row.set_trail(&chevron);
                set_opens(&row, spec.head);
                row.connect_clicked(glib::clone!(
                    #[weak(rename_to = popover)]
                    self,
                    #[weak]
                    holder,
                    move |_| {
                        if holder.details::<gtk4::Widget>().is_none() {
                            popover.emit_by_name::<()>("activated", &[&key]);
                        }
                    }
                ));
                holder.set_head(&row);
                row
            }
        }
    }

    fn card(&self, card: &Card) -> gtk4::Box {
        let body = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        if card.bar {
            let bar = gtk4::ProgressBar::new();
            bar.add_css_class(CAPACITY_BAR);
            bar.set_margin_start(12);
            bar.set_margin_end(12);
            bar.set_margin_top(6);
            bar.set_margin_bottom(6);
            body.append(&bar);
        }
        if let Some(id) = &card.open {
            body.append(&self.action(gettextrs::gettext("Open"), "activated", id));
        }
        match &card.release {
            Some(Release::Eject(id)) => {
                body.append(&self.action(gettextrs::gettext("Eject"), "eject", id))
            }
            Some(Release::Unmount(id)) => {
                body.append(&self.action(gettextrs::gettext("Unmount"), "unmount", id))
            }
            None => {}
        }
        if !card.facts.is_empty() {
            let facts = FactList::new();
            facts.set_facts(&card.facts);
            body.append(&facts);
        }
        body
    }

    fn action(&self, title: String, signal: &'static str, id: &str) -> Row {
        let row = Row::new();
        row.set_title(Some(title.as_str()));
        let id = id.to_owned();
        row.connect_clicked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>(signal, &[&id])
        ));
        row
    }
}

fn set_opens(row: &Row, head: Head) {
    let opens = head == Head::Opens;
    let activatable = matches!(head, Head::Opens | Head::Acts);
    if row.activatable() != activatable {
        row.set_activatable(activatable);
    }
    if let Some(chevron) = row.trail()
        && chevron.get_visible() != opens
    {
        chevron.set_visible(opens);
    }
}

fn release(busy: bool, release: Option<Release>) -> Option<Release> {
    release.filter(|_| !busy)
}

fn drive_row(drive: &Drive) -> DeviceRow {
    let card = match drive.dimmed {
        true => Card::default(),
        false => Card {
            release: release(
                drive.busy,
                drive.ejectable.then(|| Release::Eject(drive.id.clone())),
            ),
            facts: drive.facts.clone(),
            ..Card::default()
        },
    };
    DeviceRow {
        id: drive.id.clone(),
        title: drive.title.clone(),
        subtitle: drive.subtitle.clone(),
        icon: drive.icon.clone(),
        head: match card.is_empty() {
            true => Head::Inert,
            false => Head::Opens,
        },
        fraction: None,
        busy: drive.busy,
        dimmed: drive.dimmed,
        warning: false,
        indent: false,
        card,
    }
}

fn volume_row(id: String, volume: &Volume, busy: bool, releases: Option<Release>) -> DeviceRow {
    let card = Card {
        bar: volume.mounted && volume.fraction.is_some(),
        open: volume.mounted.then(|| id.clone()),
        release: release(busy, releases),
        facts: volume.facts.clone(),
    };
    DeviceRow {
        head: match (volume.mounted, card.is_empty()) {
            (true, _) => Head::Opens,
            (false, false) => Head::Split,
            (false, true) => Head::Acts,
        },
        id,
        title: volume.title.clone(),
        subtitle: volume.subtitle.clone(),
        icon: volume.icon.clone(),
        fraction: volume.fraction,
        busy,
        dimmed: false,
        warning: volume.warning,
        indent: false,
        card,
    }
}

fn flatten_devices(drives: &[Drive]) -> Vec<DeviceRow> {
    let mut rows = Vec::new();
    for drive in drives {
        match drive.volumes.as_slice() {
            [] => rows.push(drive_row(drive)),
            [volume] => {
                let eject = drive.ejectable.then(|| Release::Eject(drive.id.clone()));
                let unmount = volume.mounted.then(|| Release::Unmount(volume.id.clone()));
                rows.push(volume_row(
                    drive.id.clone(),
                    volume,
                    drive.busy || volume.busy,
                    eject.or(unmount),
                ));
            }
            volumes => {
                rows.push(drive_row(drive));
                for volume in volumes {
                    let mut row = volume_row(
                        format!("{}/{}", drive.id, volume.id),
                        volume,
                        volume.busy,
                        volume.mounted.then(|| Release::Unmount(volume.id.clone())),
                    );
                    row.indent = true;
                    rows.push(row);
                }
            }
        }
    }
    rows
}
