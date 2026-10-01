use super::*;

pub(super) fn check() {
    let system_monitor = SystemMonitorPopover::new();
    let tile = |id: &str, fraction| SystemMonitorUsage {
        id: id.to_owned(),
        title: id.to_owned(),
        value: "1".to_owned(),
        fraction,
        severity: None,
    };
    system_monitor.set_usage(&[tile("a", Some(0.5)), tile("b", None)]);
    assert_eq!(children(&*system_monitor.imp().usage_rows), 2);
    system_monitor.set_usage(&[tile("a", Some(0.5))]);
    assert_eq!(
        children(&*system_monitor.imp().usage_rows),
        1,
        "a tile whose key stops being passed must not remain"
    );
}
