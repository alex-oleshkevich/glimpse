mod imp;
mod row;

pub use row::InhibitorRow;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Row, none_if_empty};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum InhibitorSource {
    #[default]
    ScreenSaver,
    Portal,
    Login1,
    ManualHold,
}

impl InhibitorSource {
    fn icon_name(self) -> &'static str {
        match self {
            Self::ScreenSaver => "preferences-desktop-screensaver-symbolic",
            Self::Portal => "package-x-generic-symbolic",
            Self::Login1 => "system-run-symbolic",
            Self::ManualHold => "alarm-symbolic",
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct InhibitorTargets {
    pub idle: bool,
    pub suspend: bool,
    pub shutdown: bool,
    pub lid_switch: bool,
    pub power_key: bool,
    pub suspend_key: bool,
    pub hibernate_key: bool,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct InhibitorEntry {
    pub id: u64,
    pub source: InhibitorSource,
    pub label: String,
    pub status: String,
    pub targets: InhibitorTargets,
    pub can_release: bool,
}

glib::wrapper! {
    pub struct InhibitorList(ObjectSubclass<imp::InhibitorList>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for InhibitorList {
    fn default() -> Self {
        Self::new()
    }
}

impl InhibitorList {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_inhibitors(&self, entries: &[InhibitorEntry]) {
        let imp = self.imp();
        if imp.entries.borrow().as_slice() == entries {
            return;
        }
        imp.entries.replace(entries.to_vec());
        self.render();
    }

    pub fn connect_release_requested<F: Fn(&Self, u64) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "release-requested",
            false,
            glib::closure_local!(move |list: Self, id: u64| f(&list, id)),
        )
    }

    fn render(&self) {
        let imp = self.imp();
        let entries = imp.entries.borrow();
        let mut rows = imp.rows.borrow_mut();

        for (index, entry) in entries.iter().enumerate() {
            if rows.len() == index {
                let row = self.build_row();
                row.insert_after(self, rows.last());
                rows.push(row);
            }
            let row = &rows[index];
            row.set_id(entry.id);
            let item: &Row = row.upcast_ref();
            item.set_title(none_if_empty(&entry.label));
            item.set_subtitle(none_if_empty(&entry.status));
            item.set_lead_icon(Some(entry.source.icon_name()));
            row.set_targets(&entry.targets);
            row.set_can_release(entry.can_release);
            row.sync_accessible_label();
        }

        for row in rows.split_off(entries.len()) {
            row.unparent();
        }
    }

    fn build_row(&self) -> InhibitorRow {
        let row = InhibitorRow::new();
        row.connect_release_requested(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_, id| list.emit_by_name::<()>("release-requested", &[&id])
        ));
        row
    }
}
