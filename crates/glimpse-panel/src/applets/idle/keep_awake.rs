use std::collections::BTreeMap;
use std::rc::Rc;

use chrono::{DateTime, Local};
use gettextrs::gettext;
use glimpse_dbus::idle::{IdleInhibitorRecord, IdleProviderError, IdleProviderHandle, SourceKind};

use crate::applet::Opener;

pub type Failure = Rc<dyn Fn(&'static str, IdleProviderError)>;

pub fn awake_until(time: &str) -> String {
    gettext("Awake until {time}").replace("{time}", time)
}

pub fn manual_holds(records: &[IdleInhibitorRecord]) -> Vec<&IdleInhibitorRecord> {
    records
        .iter()
        .filter(|record| {
            record.can_release
                && record.source.kind == SourceKind::Login1
                && record.who == "glimpse-idle"
                && record.why == "Manual hold"
        })
        .collect()
}

pub fn deadline(
    records: &[IdleInhibitorRecord],
    expiries: &BTreeMap<u64, u64>,
) -> Option<(DateTime<Local>, u32)> {
    let record = manual_holds(records)
        .into_iter()
        .max_by_key(|record| (record.added_at_unix, record.id))?;
    let expires_at_unix = *expiries.get(&record.id)?;
    let seconds = u32::try_from(expires_at_unix.checked_sub(record.added_at_unix)?).ok()?;
    let at = DateTime::from_timestamp(i64::try_from(expires_at_unix).ok()?, 0)?;
    Some((at.with_timezone(&Local), seconds))
}

pub fn request(
    idle: &IdleProviderHandle,
    opener: Opener,
    seconds: u32,
    collapse: impl FnOnce() + 'static,
    failed: Failure,
) {
    let idle = idle.clone();
    relm4::spawn_local(async move {
        match idle.hold(seconds).await {
            Ok(new) => {
                for id in hold_ids_to_release(&idle.snapshot().inhibitors, Some(new)) {
                    if let Err(error) = release(&idle, id).await {
                        failed("idle.release", error);
                    }
                }
                collapse();
            }
            Err(error) => failed("idle.hold", error),
        }
        opener.wake();
    });
}

pub fn hold_ids_to_release(snapshot: &[IdleInhibitorRecord], keep: Option<u64>) -> Vec<u64> {
    let mut ids: Vec<_> = manual_holds(snapshot)
        .into_iter()
        .map(|record| record.id)
        .filter(|id| Some(*id) != keep)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

pub fn toggle(idle: &IdleProviderHandle, opener: Opener, on: bool, failed: Failure) {
    if on {
        request(idle, opener, 0, || {}, failed);
        return;
    }
    for id in hold_ids_to_release(&idle.snapshot().inhibitors, None) {
        let idle = idle.clone();
        settling(
            "idle.release",
            opener.clone(),
            async move { release(&idle, id).await },
            Rc::clone(&failed),
        );
    }
}

async fn release(idle: &IdleProviderHandle, id: u64) -> Result<(), IdleProviderError> {
    if !manual_holds(&idle.snapshot().inhibitors)
        .iter()
        .any(|record| record.id == id)
    {
        return Ok(());
    }
    release_result(idle.release(id).await)
}

fn release_result(result: Result<(), IdleProviderError>) -> Result<(), IdleProviderError> {
    match result {
        Err(IdleProviderError::UnknownObject) => Ok(()),
        result => result,
    }
}

pub fn log_failure(operation: &'static str, error: IdleProviderError) {
    tracing::warn!(operation, %error, "service command failed");
}

pub fn settling<F, T>(operation: &'static str, opener: Opener, future: F, failed: Failure)
where
    F: std::future::Future<Output = Result<T, IdleProviderError>> + 'static,
    T: 'static,
{
    relm4::spawn_local(async move {
        if let Err(error) = future.await {
            failed(operation, error);
            opener.wake();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_dbus::idle::{IdleInhibitorSource, InhibitionTargets, Login1Mode};

    fn manual(id: u64) -> IdleInhibitorRecord {
        IdleInhibitorRecord {
            id,
            who: "glimpse-idle".to_owned(),
            why: "Manual hold".to_owned(),
            bus_name: String::new(),
            process_name: String::new(),
            source: IdleInhibitorSource::login1(0, 0, Login1Mode::Block),
            targets: InhibitionTargets::manual_hold(),
            can_release: true,
            added_at_unix: id,
        }
    }

    #[test]
    fn a_new_hold_replaces_every_observed_manual_hold_once() {
        let foreign = IdleInhibitorRecord {
            who: "Firefox".to_owned(),
            ..manual(4)
        };
        let snapshot = [manual(1), manual(2), manual(3), manual(3), foreign];
        assert_eq!(hold_ids_to_release(&snapshot, Some(2)), [1, 3]);
        assert_eq!(hold_ids_to_release(&snapshot, None), [1, 2, 3]);
        assert!(hold_ids_to_release(&[], None).is_empty());
    }

    #[test]
    fn already_gone_is_a_successful_release() {
        assert!(release_result(Err(IdleProviderError::UnknownObject)).is_ok());
        assert!(release_result(Err(IdleProviderError::TimedOut)).is_err());
    }

    #[test]
    fn newest_manual_hold_controls_the_deadline() {
        let timed = manual(1);
        let expiries = BTreeMap::from([(1, 901), (3, 1_803)]);
        assert_eq!(
            deadline(std::slice::from_ref(&timed), &expiries).unwrap().1,
            900
        );
        assert_eq!(deadline(&[timed.clone(), manual(2)], &expiries), None);
        assert_eq!(deadline(&[timed, manual(3)], &expiries).unwrap().1, 1_800);
        assert_eq!(deadline(&[manual(1)], &BTreeMap::new()), None);
    }
}
