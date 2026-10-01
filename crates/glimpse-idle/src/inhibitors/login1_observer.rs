use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use glimpse_dbus::idle::{IdleInhibitorRecord, IdleInhibitorSource, InhibitionTargets, Login1Mode};
use glimpse_dbus::login1::{Login1InhibitorEntry, Login1ManagerProxy};
use tokio_util::sync::CancellationToken;

use super::{Backend, Health, SharedRegistry, WHO_CAP, WHY_CAP, unix_now};

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
        who: glimpse_utils::clean(who, WHO_CAP),
        why: glimpse_utils::clean(why, WHY_CAP),
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
    Some(glimpse_utils::clean(&comm, WHO_CAP))
}

pub async fn start(
    login1: Option<Login1ManagerProxy<'static>>,
    registry: Arc<SharedRegistry>,
    health: Health,
    cancel: CancellationToken,
) {
    let Some(login1) = login1 else {
        tracing::warn!("no system bus; login1 inhibitors will not be observed");
        health.degraded(Backend::Login1, "cannot reach org.freedesktop.login1");
        return;
    };

    let own_pid = std::process::id();
    let mut observed: HashMap<ObservedKey, ObservedInhibitor> = HashMap::new();
    let mut interval = tokio::time::interval(POLL_INTERVAL);
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
                health.degraded(Backend::Login1, error.to_string());
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

        health.ready(Backend::Login1);
    }
}

#[cfg(test)]
mod tests;
