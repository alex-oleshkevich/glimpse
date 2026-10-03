use super::*;

pub(super) fn check() {
    let system_monitor = SystemMonitorPopover::new();
    let disk = |title: &str| SystemMonitorNamedMeter {
        title: title.to_owned(),
        meter: SystemMonitorMeter {
            level: 0.5,
            value: "1".to_owned(),
            severity: None,
        },
    };
    let readings = |disks| SystemMonitorReadings {
        disks,
        ..Default::default()
    };
    system_monitor.set_readings(&readings(vec![disk("/"), disk("/home")]));
    assert_eq!(children(&*system_monitor.imp().disks), 2);
    assert!(!system_monitor.imp().cpu.get_visible());
    system_monitor.set_readings(&readings(vec![disk("/")]));
    assert_eq!(
        children(&*system_monitor.imp().disks),
        1,
        "a disk whose path stops being passed must not remain"
    );
}
