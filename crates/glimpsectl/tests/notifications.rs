use chrono::{DateTime, TimeZone as _, Timelike as _, Utc};
use chrono::{Local, NaiveTime, TimeDelta};
use glimpse_dbus::notifications::{NotificationRecord, NotificationUrgency};
use glimpsectl::commands::notifications::{INDEFINITELY, expiry, row};

#[test]
fn an_expiry_is_the_next_time_the_clock_reads_that_way() {
    for (hour, minute) in [(0, 0), (7, 30), (12, 0), (23, 59)] {
        let time = NaiveTime::from_hms_opt(hour, minute, 0).expect("a valid time");
        let micros = expiry(true, Some(time)).expect("it arrives within two days");
        let when = DateTime::from_timestamp_micros(micros)
            .expect("the provider can read it back")
            .with_timezone(&Local);

        assert!(when > Local::now(), "{time} resolved into the past");
        assert!(when < Local::now() + TimeDelta::days(2));
        assert_eq!((when.hour(), when.minute()), (hour, minute));
    }
}

#[test]
fn no_expiry_is_the_sentinel_the_provider_reads_as_indefinite() {
    assert_eq!(expiry(true, None).expect("on, forever"), INDEFINITELY);
    assert_eq!(expiry(false, None).expect("off"), INDEFINITELY);
}

#[test]
fn an_expiry_on_a_command_turning_it_off_is_refused_rather_than_ignored() {
    let time = NaiveTime::from_hms_opt(21, 30, 0).expect("a valid time");
    let error = expiry(false, Some(time)).expect_err("there is nothing to lapse");

    assert!(error.to_string().contains("dnd on"), "{error}");
}

fn at(hour: u32, minute: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 14, hour, minute, 0)
        .single()
        .expect("a valid instant")
}

#[test]
fn an_unread_notification_is_not_dimmed_and_a_read_one_is() {
    let mut record = NotificationRecord {
        id: 7,
        app_id: "org.example.App".to_owned(),
        app_name: String::new(),
        app_pid: None,
        summary: "Build finished".to_owned(),
        body: None,
        icon: None,
        image: None,
        urgency: NotificationUrgency::Normal,
        actions: Vec::new(),
        progress: None,
        created: at(20, 0),
        unread: true,
        resident: false,
        expire_timeout: -1,
    };

    let [id, app, _, summary] = row(&record);
    assert_eq!(id, "7");
    assert_eq!(
        app, "org.example.App",
        "a nameless sender falls back to its id"
    );
    assert_eq!(summary, "Build finished");

    record.unread = false;
    let [_, _, _, dimmed] = row(&record);
    assert!(dimmed.contains("Build finished"));
    assert_ne!(dimmed, "Build finished", "a read summary is styled");
}
