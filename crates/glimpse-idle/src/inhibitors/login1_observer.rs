use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use glimpse_dbus::idle::{
    BackendHealth, HealthKind, IdleInhibitorRecord, IdleInhibitorSource, InhibitionTargets,
    Login1Mode,
};
use glimpse_dbus::login1::{Login1InhibitorEntry, Login1ManagerProxy};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::{SharedRegistry, WHO_CAP, WHY_CAP, clamp_label, unix_now};

const POLL_INTERVAL: Duration = Duration::from_secs(5);

pub type ObservedKey = (u32, String, String);

pub struct ObservedInhibitor {
    pub id: u64,
    pub process_name: String,
}

pub struct Diff {
    pub added: Vec<Login1InhibitorEntry>,
    pub removed: Vec<ObservedKey>,
}

pub struct Rename {
    pub id: u64,
    pub process_name: String,
}

pub(crate) fn set_health(
    health: &std::sync::Mutex<BackendHealth>,
    generation: &watch::Sender<u64>,
    next: BackendHealth,
) {
    let changed = {
        let mut current = health
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *current == next {
            false
        } else {
            *current = next;
            true
        }
    };
    if changed {
        generation.send_modify(|value| *value = value.wrapping_add(1));
    }
}

pub fn parse_login1_mode(mode: &str) -> Option<Login1Mode> {
    match mode {
        "block" => Some(Login1Mode::Block),
        "block-weak" => Some(Login1Mode::BlockWeak),
        "delay" => Some(Login1Mode::Delay),
        _ => None,
    }
}

fn observed_key(entry: &Login1InhibitorEntry) -> ObservedKey {
    (entry.5, entry.1.clone(), entry.2.clone())
}

fn is_surfaced(entry: &Login1InhibitorEntry, own_pid: u32) -> bool {
    if entry.5 == own_pid {
        return false;
    }
    match parse_login1_mode(&entry.3) {
        Some(Login1Mode::Block | Login1Mode::BlockWeak) => true,
        Some(Login1Mode::Delay) => false,
        None => {
            tracing::warn!(mode = %entry.3, "logind reported an unrecognized inhibitor mode; skipping it");
            false
        }
    }
}

pub fn compute_diff(
    observed: &HashMap<ObservedKey, ObservedInhibitor>,
    current: &[Login1InhibitorEntry],
    own_pid: u32,
) -> Diff {
    let surfaced: Vec<&Login1InhibitorEntry> = current
        .iter()
        .filter(|entry| is_surfaced(entry, own_pid))
        .collect();
    let current_keys: HashSet<ObservedKey> =
        surfaced.iter().map(|entry| observed_key(entry)).collect();

    // logind hands out no id of its own, so two genuinely distinct inhibitors from the same
    // process sharing a `who`/`why` pair are indistinguishable by key within one snapshot; the
    // first occurrence wins and the rest are dropped rather than silently overwriting each other
    // in `observed`, which would otherwise leak the overwritten one as an untracked, permanently
    // un-releasable registry record.
    let mut seen = HashSet::with_capacity(surfaced.len());
    let added = surfaced
        .into_iter()
        .filter(|entry| {
            let key = observed_key(entry);
            !observed.contains_key(&key) && seen.insert(key)
        })
        .cloned()
        .collect();

    let removed = observed
        .keys()
        .filter(|key| !current_keys.contains(*key))
        .cloned()
        .collect();

    Diff { added, removed }
}

pub fn entry_to_record(
    entry: &Login1InhibitorEntry,
    id: u64,
    process_name: String,
) -> IdleInhibitorRecord {
    let (what, who, why, mode, uid, pid) = entry;
    IdleInhibitorRecord {
        id,
        who: clamp_label(who, WHO_CAP),
        why: clamp_label(why, WHY_CAP),
        bus_name: String::new(),
        process_name,
        source: IdleInhibitorSource::login1(
            *pid,
            *uid,
            parse_login1_mode(mode).unwrap_or_default(),
        ),
        targets: InhibitionTargets::from_login1_what(what),
        can_release: false,
        added_at_unix: unix_now(),
    }
}

pub fn compute_renames(
    observed: &HashMap<ObservedKey, ObservedInhibitor>,
    fresh_names: &HashMap<ObservedKey, String>,
) -> Vec<Rename> {
    observed
        .iter()
        .filter_map(|(key, inhibitor)| {
            let fresh = fresh_names.get(key)?;
            (*fresh != inhibitor.process_name).then(|| Rename {
                id: inhibitor.id,
                process_name: fresh.clone(),
            })
        })
        .collect()
}

async fn read_process_name(pid: u32) -> Option<String> {
    let comm = tokio::fs::read_to_string(format!("/proc/{pid}/comm"))
        .await
        .ok()?;
    Some(clamp_label(&comm, WHO_CAP))
}

pub async fn start(
    login1: Option<Login1ManagerProxy<'static>>,
    registry: Arc<SharedRegistry>,
    health: Arc<std::sync::Mutex<BackendHealth>>,
    health_generation: watch::Sender<u64>,
    cancel: CancellationToken,
) {
    let Some(login1) = login1 else {
        tracing::warn!("no system bus; login1 inhibitors will not be observed");
        set_health(
            &health,
            &health_generation,
            BackendHealth {
                kind: HealthKind::Degraded,
                message: "cannot reach org.freedesktop.login1".to_owned(),
            },
        );
        return;
    };

    let own_pid = std::process::id();
    let mut observed: HashMap<ObservedKey, ObservedInhibitor> = HashMap::new();
    let mut interval = tokio::time::interval(POLL_INTERVAL);
    // A hung `ListInhibitors` reply delays the next tick already; a burst of catch-up ticks the
    // instant it finally fails or returns would turn one slow poll into a retry storm.
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut poll_failing = false;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = interval.tick() => {}
        }

        let entries = match login1.list_inhibitors().await {
            Ok(entries) => {
                poll_failing = false;
                entries
            }
            Err(error) => {
                if poll_failing {
                    tracing::debug!(%error, "logind ListInhibitors still failing");
                } else {
                    tracing::warn!(%error, "logind ListInhibitors failed");
                    poll_failing = true;
                }
                set_health(
                    &health,
                    &health_generation,
                    BackendHealth {
                        kind: HealthKind::Degraded,
                        message: glimpse_utils::clean(&error.to_string(), WHY_CAP),
                    },
                );
                continue;
            }
        };

        let diff = compute_diff(&observed, &entries, own_pid);

        for entry in &diff.added {
            let process_name = read_process_name(entry.5).await.unwrap_or_default();
            let id = registry.mutate(|registry| registry.mint_id()).await;
            let record = entry_to_record(entry, id, process_name.clone());
            registry
                .mutate(|registry| registry.insert(record, None))
                .await;
            observed.insert(observed_key(entry), ObservedInhibitor { id, process_name });
        }

        for key in &diff.removed {
            if let Some(inhibitor) = observed.remove(key) {
                registry
                    .mutate(|registry| registry.release_record(inhibitor.id))
                    .await;
            }
        }

        // A read that fails (the process exited between the `ListInhibitors` reply and this read)
        // is left out of `fresh_names` entirely, so `compute_renames`'s own "no fresh name" guard
        // skips it — never a blank overwrite of a name we already have on file.
        let mut fresh_names = HashMap::with_capacity(observed.len());
        for key in observed.keys() {
            if let Some(process_name) = read_process_name(key.0).await {
                fresh_names.insert(key.clone(), process_name);
            }
        }
        for rename in compute_renames(&observed, &fresh_names) {
            registry
                .mutate(|registry| {
                    registry.set_process_name(rename.id, rename.process_name.clone())
                })
                .await;
            if let Some(inhibitor) = observed
                .values_mut()
                .find(|inhibitor| inhibitor.id == rename.id)
            {
                inhibitor.process_name = rename.process_name;
            }
        }

        set_health(
            &health,
            &health_generation,
            BackendHealth {
                kind: HealthKind::Ready,
                message: String::new(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(what: &str, who: &str, why: &str, mode: &str, pid: u32) -> Login1InhibitorEntry {
        (what.into(), who.into(), why.into(), mode.into(), 1000, pid)
    }

    /// AC-1: additions are detected and the daemon's own pid is excluded.
    #[test]
    fn diff_detects_additions_excluding_own_pid() {
        let previous = HashMap::new();
        let current = vec![
            entry("idle", "firefox", "video", "block", 1234),
            entry("idle:sleep", "glimpse-idle", "Manual hold", "block", 9999),
        ];

        let diff = compute_diff(&previous, &current, 9999);

        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].1, "firefox");
        assert!(diff.removed.is_empty());
    }

    /// A single snapshot carrying two entries with the same `(pid, who, why)` key but different
    /// `what`/`mode` (the same process taking two distinct inhibitors under identical labels) must
    /// not both be added — `observed` can only ever map that key to one id, so a second `insert`
    /// under the same key would silently orphan the first as an untracked, un-releasable record.
    /// The first occurrence wins.
    #[test]
    fn diff_deduplicates_same_key_entries_within_one_snapshot() {
        let previous = HashMap::new();
        let current = vec![
            entry("idle", "backup-tool", "snapshotting", "block", 4242),
            entry("sleep", "backup-tool", "snapshotting", "block-weak", 4242),
        ];

        let diff = compute_diff(&previous, &current, 9999);

        assert_eq!(
            diff.added.len(),
            1,
            "the duplicate key must collapse to one addition"
        );
        assert_eq!(
            diff.added[0].0, "idle",
            "the first occurrence must be the one kept"
        );
    }

    /// AC-2: delay-mode entries never surface; block and block-weak both do.
    #[test]
    fn diff_filters_delay_mode_but_keeps_block_and_block_weak() {
        let previous = HashMap::new();
        let current = vec![
            entry("idle", "firefox", "video", "block", 1234),
            entry("sleep", "NetworkManager", "cleanup", "delay", 555),
            entry("shutdown", "ModemManager", "cleanup", "delay", 666),
            entry("idle", "apt", "upgrade", "block-weak", 777),
        ];

        let diff = compute_diff(&previous, &current, 9999);

        let who: Vec<&str> = diff.added.iter().map(|entry| entry.1.as_str()).collect();
        assert!(who.contains(&"firefox"));
        assert!(who.contains(&"apt"));
        assert!(!who.contains(&"NetworkManager"));
        assert!(!who.contains(&"ModemManager"));
    }

    /// A mode logind's own vocabulary does not define is never trusted; the entry is dropped.
    #[test]
    fn diff_drops_entries_with_an_unrecognized_mode() {
        let previous = HashMap::new();
        let current = vec![entry("idle", "mystery", "why", "bogus-mode", 1234)];

        let diff = compute_diff(&previous, &current, 9999);

        assert!(diff.added.is_empty());
    }

    /// AC-3: an entry present in the previous poll and absent now is reported removed.
    #[test]
    fn diff_detects_removals() {
        let mut previous = HashMap::new();
        previous.insert(
            (1234u32, "firefox".to_owned(), "video".to_owned()),
            ObservedInhibitor {
                id: 1,
                process_name: "firefox".into(),
            },
        );
        previous.insert(
            (5678u32, "apt".to_owned(), "upgrade".to_owned()),
            ObservedInhibitor {
                id: 2,
                process_name: "apt".into(),
            },
        );
        let current = vec![entry("idle", "firefox", "video", "block", 1234)];

        let diff = compute_diff(&previous, &current, 9999);

        assert!(diff.added.is_empty());
        assert_eq!(
            diff.removed,
            vec![(5678u32, "apt".to_owned(), "upgrade".to_owned())]
        );
    }

    #[test]
    fn parse_login1_mode_maps_the_three_known_strings_and_nothing_else() {
        assert_eq!(parse_login1_mode("block"), Some(Login1Mode::Block));
        assert_eq!(parse_login1_mode("block-weak"), Some(Login1Mode::BlockWeak));
        assert_eq!(parse_login1_mode("delay"), Some(Login1Mode::Delay));
        assert_eq!(parse_login1_mode("anything-else"), None);
    }

    /// AC-4: pid/uid/targets carry through and can_release is always false.
    #[test]
    fn entry_to_record_carries_pid_uid_and_targets_and_never_allows_release() {
        let e = entry("sleep:shutdown", "apt", "upgrade", "block", 1234);
        let record = entry_to_record(&e, 42, "apt".to_owned());

        assert_eq!(record.id, 42);
        assert_eq!(record.who, "apt");
        assert_eq!(record.why, "upgrade");
        assert_eq!(record.process_name, "apt");
        assert!(record.targets.suspend);
        assert!(record.targets.shutdown);
        assert!(!record.targets.idle);
        assert!(!record.can_release);
        assert_eq!(record.bus_name, "");
        assert_eq!(record.source.kind, glimpse_dbus::idle::SourceKind::Login1);
        assert_eq!(record.source.pid, 1234);
        assert_eq!(record.source.uid, 1000);
        assert_eq!(record.source.mode, Login1Mode::Block);
    }

    /// AC-5: a rename is only reported when the freshly-read name actually differs.
    #[test]
    fn compute_renames_only_reports_ids_whose_process_name_actually_changed() {
        let mut observed = HashMap::new();
        let unchanged_key = (1234u32, "firefox".to_owned(), "video".to_owned());
        let changed_key = (5678u32, "apt".to_owned(), "upgrade".to_owned());
        observed.insert(
            unchanged_key.clone(),
            ObservedInhibitor {
                id: 1,
                process_name: "firefox".into(),
            },
        );
        observed.insert(
            changed_key.clone(),
            ObservedInhibitor {
                id: 2,
                process_name: "apt".into(),
            },
        );

        let mut fresh_names = HashMap::new();
        fresh_names.insert(unchanged_key, "firefox".to_owned());
        fresh_names.insert(changed_key, "dpkg".to_owned());

        let renames = compute_renames(&observed, &fresh_names);

        assert_eq!(renames.len(), 1);
        assert_eq!(renames[0].id, 2);
        assert_eq!(renames[0].process_name, "dpkg");
    }

    #[test]
    fn compute_renames_reports_nothing_when_no_fresh_name_is_available() {
        let mut observed = HashMap::new();
        observed.insert(
            (1234u32, "firefox".to_owned(), "video".to_owned()),
            ObservedInhibitor {
                id: 1,
                process_name: "firefox".into(),
            },
        );

        let renames = compute_renames(&observed, &HashMap::new());

        assert!(renames.is_empty());
    }

    /// A pid with no readable `/proc/<pid>/comm` (already exited, or never existed) must not be
    /// blamed with a blank name — the caller feeds this `None` straight past `fresh_names`, never
    /// converting it into an empty-string overwrite.
    #[tokio::test]
    async fn read_process_name_returns_none_for_a_pid_with_no_readable_comm() {
        assert_eq!(read_process_name(u32::MAX).await, None);
    }
}
