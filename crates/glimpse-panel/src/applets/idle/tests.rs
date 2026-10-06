use chrono::DateTime;
use glimpse_dbus::idle::{
    IdleInhibitorRecord, IdleInhibitorSource, IdleProvider, InhibitionTargets, Login1Mode,
};

use super::*;

#[test]
fn external_inhibitors_leave_the_indicator_idle() {
    let provider = IdleProvider::unavailable("idle provider unavailable");
    let mut applet = Idle::start(provider.handle());
    applet.state.available = true;
    applet.state.reason = None;
    applet.state.inhibitors.push(IdleInhibitorRecord {
        id: 1,
        who: "Firefox".to_owned(),
        why: "Playing video".to_owned(),
        bus_name: ":1.7".to_owned(),
        process_name: "firefox".to_owned(),
        source: IdleInhibitorSource::screen_saver(1),
        targets: InhibitionTargets::idle_only(),
        can_release: false,
        added_at_unix: 0,
    });

    let indicator = applet.indicator().unwrap();

    assert_eq!(indicator.severity, None);
    assert_eq!(
        applet.icon.as_ref().map(|(name, _)| *name),
        Some(render::ICON_IDLE)
    );
}

#[test]
fn manual_hold_activates_the_indicator() {
    let provider = IdleProvider::unavailable("idle provider unavailable");
    let mut applet = Idle::start(provider.handle());
    applet.state.available = true;
    applet.state.reason = None;
    let added_at_unix = Local::now().timestamp() as u64;
    applet.state.inhibitors.push(IdleInhibitorRecord {
        id: 1,
        who: "glimpse-idle".to_owned(),
        why: "Manual hold".to_owned(),
        bus_name: ":1.7".to_owned(),
        process_name: String::new(),
        source: IdleInhibitorSource::login1(4242, 1000, Login1Mode::Block),
        targets: InhibitionTargets::manual_hold(),
        can_release: true,
        added_at_unix,
    });
    applet.state.hold_expiries.insert(1, added_at_unix + 3_600);

    let indicator = applet.indicator().unwrap();

    assert_eq!(indicator.severity, Some(Severity::Warning));
    assert_eq!(
        applet.icon.as_ref().map(|(name, _)| *name),
        Some(render::ICON_ACTIVE)
    );
    let at = DateTime::from_timestamp((added_at_unix + 3_600) as i64, 0)
        .unwrap()
        .with_timezone(&Local);
    let ends = at.format(glimpse_config::clock(false)).to_string();
    assert_eq!(applet.ends(), Some(ends.clone()));
    assert_eq!(indicator.tooltip, Some(render::awake_until(&ends)));
    assert_eq!(applet.left(), Some(render::time_left(60)));

    applet.state.inhibitors.clear();
    assert_eq!(applet.ends(), None);
    assert_eq!(applet.left(), None);
    assert_eq!(applet.indicator().unwrap().severity, None);
}
