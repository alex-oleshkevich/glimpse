mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Row, Severity, none_if_empty, reconcile, set_css_class, set_footer_row};

pub use imp::{DetailTile, UsageTile};

const USAGE_BAR: &str = "system-monitor-usage-bar";
const WARNING: &str = "system-monitor-usage-bar--warning";
const ERROR: &str = "system-monitor-usage-bar--error";

glib::wrapper! {
    pub struct SystemMonitorPopover(ObjectSubclass<imp::SystemMonitorPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for SystemMonitorPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemMonitorPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_usage(&self, tiles: &[UsageTile]) {
        let imp = self.imp();
        if imp.usage_data.borrow().as_slice() == tiles {
            return;
        }
        imp.usage_data.replace(tiles.to_vec());
        imp.usage.set_visible(!tiles.is_empty());
        reconcile::by_key(
            &*imp.usage_rows,
            &mut imp.usage_held.borrow_mut(),
            tiles,
            |tile| tile.id.clone(),
            |_tile| gtk4::Box::new(gtk4::Orientation::Vertical, 0),
            apply_usage_cell,
        );
    }

    pub fn set_details(&self, tiles: &[DetailTile]) {
        let imp = self.imp();
        if imp.details_data.borrow().as_slice() == tiles {
            return;
        }
        imp.details_data.replace(tiles.to_vec());
        imp.details.set_visible(!tiles.is_empty());
        reconcile::by_key(
            &*imp.details_rows,
            &mut imp.details_held.borrow_mut(),
            tiles,
            |tile| tile.id.clone(),
            |_tile| Row::new(),
            |row, tile| {
                row.set_activatable(false);
                row.set_title(none_if_empty(&tile.title));
                row.set_value(none_if_empty(&tile.value));
            },
        );
    }

    pub fn set_footer(&self, label: Option<&str>) {
        set_footer_row(&self.imp().footer, label);
    }

    pub fn connect_footer_activated<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "footer-activated",
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }
}

fn apply_usage_cell(cell: &gtk4::Box, tile: &UsageTile) {
    let row = match cell.first_child().and_downcast::<Row>() {
        Some(row) => row,
        None => {
            let row = Row::new();
            cell.prepend(&row);
            row
        }
    };
    row.set_activatable(false);
    row.set_title(none_if_empty(&tile.title));
    row.set_value(none_if_empty(&tile.value));
    apply_usage_bar(cell, tile.fraction, tile.severity);
}

fn apply_usage_bar(cell: &gtk4::Box, fraction: Option<f64>, severity: Option<Severity>) {
    crate::progress::apply_bar(cell, fraction, USAGE_BAR);
    if let Some(bar) = cell.last_child().and_downcast::<gtk4::ProgressBar>() {
        set_css_class(&bar, WARNING, severity == Some(Severity::Warning));
        set_css_class(&bar, ERROR, severity == Some(Severity::Error));
    }
}
