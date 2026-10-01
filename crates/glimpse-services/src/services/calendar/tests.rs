use std::path::Path;

use std::sync::Arc;

use chrono::{Datelike as _, TimeZone};

use glimpse_dbus::Buses;

use tokio_util::sync::CancellationToken;

use super::*;

use crate::{ServiceState, service::ServiceRuntime};

const DOCUMENT: &str = "\
BEGIN:VCALENDAR\r
VERSION:2.0\r
PRODID:-//glimpse//test//EN\r
BEGIN:VEVENT\r
UID:one@example\r
DTSTAMP:20260901T000000Z\r
DTSTART:20260904T090000Z\r
DTEND:20260904T093000Z\r
SUMMARY:Standup\r
LOCATION:Meeting room 2\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:two@example\r
DTSTAMP:20260901T000000Z\r
DTSTART:20260907T100000Z\r
DTEND:20260907T103000Z\r
RRULE:FREQ=WEEKLY;COUNT=3\r
EXDATE:20260914T100000Z\r
SUMMARY:Weekly\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:three@example\r
DTSTAMP:20260901T000000Z\r
DTSTART;VALUE=DATE:20260905\r
DTEND;VALUE=DATE:20260906\r
SUMMARY:Holiday\r
END:VEVENT\r
END:VCALENDAR\r
";

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 4, 8, 0, 0)
        .single()
        .expect("one instant")
}

fn source(id: &str, kind: CalendarSourceKind, uri: &str) -> CalendarSource {
    CalendarSource {
        id: id.to_owned(),
        kind,
        uri: uri.to_owned(),
        name: None,
        poll_interval: None,
        color: None,
    }
}

fn document(poll_interval: u64, sources: Vec<CalendarSource>) -> glimpse_config::Config {
    glimpse_config::Config {
        calendar: glimpse_config::Calendar {
            poll_interval,
            sources,
        },
        ..Default::default()
    }
}

/// An iCalendar timestamp has no sub-second field, so an expectation built from `Utc::now()`
/// carries precision the document cannot round-trip.
fn whole_second() -> DateTime<Utc> {
    DateTime::from_timestamp(Utc::now().timestamp(), 0).expect("an instant in range")
}

fn crowded(base: DateTime<Utc>, count: usize) -> String {
    let mut document =
        String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//glimpse//test//EN\r\n");
    for index in 0..count {
        let start = base + TimeDelta::minutes(30 * index as i64);
        let end = start + TimeDelta::minutes(30);
        document.push_str(&format!(
            "BEGIN:VEVENT\r\nUID:crowd-{index}\r\nDTSTAMP:20260901T000000Z\r\n\
                 DTSTART:{}\r\nDTEND:{}\r\nSUMMARY:Crowded {index}\r\nEND:VEVENT\r\n",
            start.format("%Y%m%dT%H%M%SZ"),
            end.format("%Y%m%dT%H%M%SZ"),
        ));
    }
    document.push_str("END:VCALENDAR\r\n");
    document
}

fn window() -> Window {
    Window::around(now())
}

fn parsed(document: &str) -> Arc<ICalendar> {
    Arc::new(document.parse().expect("a parseable document"))
}

fn loaded(id: &str, calendar: Arc<ICalendar>) -> Loaded {
    Loaded {
        id: id.to_owned(),
        name: None,
        color: None,
        calendars: vec![calendar],
    }
}

fn payload(expansion: Event) -> CalendarEvents {
    let Event::Expanded { payload, .. } = expansion else {
        panic!("expanding yields an expansion");
    };
    payload
}

async fn crowd(count: usize) -> CalendarEvents {
    let root = tempfile::tempdir().expect("a scratch directory");
    tokio::fs::write(root.path().join("crowd.ics"), crowded(now(), count))
        .await
        .expect("a written fixture");
    let fetch = read(
        None,
        CalendarSourceKind::Directory,
        root.path().to_string_lossy().into_owned(),
    )
    .await
    .expect("a readable directory");

    payload(expanding(
        0,
        vec![Loaded {
            id: "crowd".to_owned(),
            name: None,
            color: None,
            calendars: fetch.calendars,
        }],
        window(),
    ))
}

fn expanded(summary: &str) -> Vec<Occurrence> {
    expand(&parsed(DOCUMENT), window())
        .into_iter()
        .filter(|occurrence| occurrence.summary == summary)
        .collect()
}

fn recurring(overrides: &str) -> Vec<String> {
    let document = format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//glimpse//test//EN\r\n\
             BEGIN:VEVENT\r\nUID:r@example\r\nDTSTAMP:20260901T000000Z\r\n\
             DTSTART:20260902T090000Z\r\nDTEND:20260902T093000Z\r\n\
             RRULE:FREQ=DAILY;COUNT=4\r\nSUMMARY:Daily\r\nEND:VEVENT\r\n\
             {overrides}END:VCALENDAR\r\n"
    );
    let mut shown: Vec<String> = expand(&parsed(&document), window())
        .iter()
        .map(|occurrence| format!("{} {}", occurrence.start, occurrence.summary))
        .collect();
    shown.sort();
    shown
}

fn one(body: &str) -> Occurrence {
    let document = format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//glimpse//test//EN\r\n\
             BEGIN:VEVENT\r\nUID:x@example\r\nDTSTAMP:20260901T000000Z\r\n\
             DTSTART:20260904T090000Z\r\nDTEND:20260904T093000Z\r\n\
             SUMMARY:Entry\r\n{body}END:VEVENT\r\nEND:VCALENDAR\r\n"
    );
    let found = expand(&parsed(&document), window());
    assert_eq!(found.len(), 1, "{body}");
    found.into_iter().next().expect("one occurrence")
}

fn none(body: &str) {
    let document = format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//glimpse//test//EN\r\n\
             BEGIN:VEVENT\r\nUID:x@example\r\nDTSTAMP:20260901T000000Z\r\n\
             DTSTART:20260904T090000Z\r\nDTEND:20260904T093000Z\r\n\
             SUMMARY:Entry\r\n{body}END:VEVENT\r\nEND:VCALENDAR\r\n"
    );
    assert!(
        expand(&parsed(&document), window()).is_empty(),
        "{body} should not publish"
    );
}

/// A fetch reaches the filesystem through tokio's blocking pool, which yielding does not
/// advance: without waiting on real time the inbox is still empty when the assertion runs.
#[derive(Default)]
struct Observation {
    states: std::sync::Mutex<Vec<CalendarEvents>>,
    health: std::sync::Mutex<Vec<ServiceState>>,
}

impl Observation {
    fn published(&self) -> Vec<CalendarEvents> {
        self.states
            .lock()
            .expect("state lock")
            .iter()
            .cloned()
            .collect()
    }

    fn health(&self) -> Vec<ServiceState> {
        self.health
            .lock()
            .expect("health lock")
            .iter()
            .cloned()
            .collect()
    }
}

struct Live {
    observation: Arc<Observation>,
    sender: crate::service::ServiceSender<Calendar>,
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl Live {
    fn start(sources: Vec<CalendarSource>) -> Self {
        let observation = Arc::new(Observation::default());
        let cancel = CancellationToken::new();
        let (mut runtime, handle) = ServiceRuntime::<Calendar>::new(
            Config {
                poll_interval: MIN_POLL,
                sources,
            },
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        );
        let mut state = handle.subscribe();
        let mut health = handle.health();
        let observed = observation.clone();
        let initial = handle.snapshot();
        tokio::spawn(async move {
            observed.states.lock().expect("state lock").push(initial);
            observed
                .health
                .lock()
                .expect("health lock")
                .push(health.borrow().clone());
            loop {
                tokio::select! {
                    changed = state.changed() => {
                        if changed.is_err() { break; }
                        observed.states.lock().expect("state lock").push(state.borrow_and_update().clone());
                    }
                    changed = health.changed() => {
                        if changed.is_err() { break; }
                        observed.health.lock().expect("health lock").push(health.borrow_and_update().clone());
                    }
                }
            }
        });
        let sender = runtime.sender();
        let handle = tokio::spawn(async move {
            let _ = runtime.run(()).await;
        });

        Self {
            observation,
            sender,
            cancel,
            handle,
        }
    }

    async fn until(&self, done: impl Fn(&Observation) -> bool) -> bool {
        for _ in 0..400 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            if done(&self.observation) {
                return true;
            }
        }
        false
    }

    async fn stop(self) -> Arc<Observation> {
        self.cancel.cancel();
        let _ = self.handle.await;
        self.observation
    }
}

async fn settled(
    sources: Vec<CalendarSource>,
    done: impl Fn(&Observation) -> bool,
) -> Arc<Observation> {
    let live = Live::start(sources);
    live.until(done).await;
    live.stop().await
}

fn marks(observation: &Observation) -> Vec<Option<DateTime<Utc>>> {
    observation
        .published()
        .into_iter()
        .map(|payload| payload.truncated_from)
        .collect()
}

fn holds(observation: &Observation, summary: &str) -> bool {
    published(observation)
        .into_iter()
        .flatten()
        .any(|event| event.summary == summary)
}

fn published(observation: &Observation) -> Vec<Vec<CalendarEvent>> {
    observation
        .published()
        .into_iter()
        .map(|payload| payload.events)
        .collect()
}

fn ics(uid: &str, summary: &str, at: DateTime<Utc>) -> String {
    let stamp = at.format("%Y%m%dT%H%M%SZ");
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//glimpse//test//EN\r\n\
             BEGIN:VEVENT\r\nUID:{uid}\r\nDTSTAMP:{stamp}\r\nDTSTART:{stamp}\r\n\
             DTEND:{stamp}\r\nSUMMARY:{summary}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    )
}

mod details;
mod expansion;
mod service;
mod sources;
mod window;
