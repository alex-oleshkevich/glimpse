mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{MeterRow, Row, RowTone, Severity, none_if_empty, reconcile};

pub use imp::{Meter, NamedMeter, Readings};

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

    pub fn set_readings(&self, readings: &Readings) {
        let imp = self.imp();
        if *imp.readings.borrow() == *readings {
            return;
        }
        imp.readings.replace(readings.clone());
        apply_meter(&imp.cpu, readings.cpu.as_ref());
        apply_meter(&imp.memory, readings.memory.as_ref());
        apply_meter(&imp.swap, readings.swap.as_ref());
        reconcile::by_key(
            &*imp.disks,
            &mut imp.disks_held.borrow_mut(),
            &readings.disks,
            |disk| disk.title.clone(),
            |_disk| MeterRow::new(),
            apply_named,
        );
        apply_meter(&imp.gpu, readings.gpu.as_ref());
        if let Some(gpu_memory) = &readings.gpu_memory {
            apply_named(&imp.gpu_memory, gpu_memory);
        }
        imp.gpu_memory.set_visible(readings.gpu_memory.is_some());
        imp.usage.set_visible(
            readings.cpu.is_some()
                || readings.memory.is_some()
                || readings.swap.is_some()
                || !readings.disks.is_empty()
                || readings.gpu.is_some()
                || readings.gpu_memory.is_some(),
        );

        apply_value(&imp.load, readings.load.as_deref());
        apply_value(&imp.uptime, readings.uptime.as_deref());
        apply_value(&imp.network, readings.network.as_deref());
        apply_value(&imp.cpu_temp, readings.cpu_temp.as_deref());
        apply_value(&imp.gpu_temp, readings.gpu_temp.as_deref());
        imp.details.set_visible(
            readings.load.is_some()
                || readings.uptime.is_some()
                || readings.network.is_some()
                || readings.cpu_temp.is_some()
                || readings.gpu_temp.is_some(),
        );
    }

}

fn apply_named(row: &MeterRow, named: &NamedMeter) {
    row.upcast_ref::<Row>()
        .set_title(none_if_empty(&named.title));
    apply_meter(row, Some(&named.meter));
}

fn apply_meter(row: &MeterRow, meter: Option<&Meter>) {
    row.set_visible(meter.is_some());
    let Some(meter) = meter else {
        return;
    };
    row.set_level(meter.level);
    let row = row.upcast_ref::<Row>();
    row.set_value(none_if_empty(&meter.value));
    row.set_tone(match meter.severity {
        Some(Severity::Warning) => RowTone::Warning,
        Some(Severity::Error) => RowTone::Error,
        _ => RowTone::Normal,
    });
}

fn apply_value(row: &Row, value: Option<&str>) {
    row.set_visible(value.is_some());
    row.set_value(value);
}
