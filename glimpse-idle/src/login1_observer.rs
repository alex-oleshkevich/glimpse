use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use glimpse_core::dbus::login1::{Login1InhibitorEntry, Login1ManagerProxy};
use glimpse_core::services::idle_inhibitor::{
    IdleInhibitorRecord, IdleInhibitorSource, InhibitionTargets, Login1Mode, now_unix,
};

use crate::inhibitor_registry::Registry;

const POLL_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Login1Key {
    pub pid: u32,
    pub who: String,
    pub why: String,
}

pub fn parse_mode(s: &str) -> Login1Mode {
    match s {
        "delay" => Login1Mode::Delay,
        "block-weak" => Login1Mode::BlockWeak,
        _ => Login1Mode::Block,
    }
}

pub fn key_of(e: &Login1InhibitorEntry) -> Login1Key {
    Login1Key { pid: e.5, who: e.1.clone(), why: e.2.clone() }
}

pub fn read_proc_comm(pid: u32) -> String {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

pub fn entry_to_record(id: u64, e: &Login1InhibitorEntry) -> IdleInhibitorRecord {
    let (what, who, why, mode, uid, pid) = e.clone();
    IdleInhibitorRecord {
        id,
        who,
        why,
        bus_name: String::new(),
        process_name: read_proc_comm(pid),
        source: IdleInhibitorSource::login1(pid, uid, parse_mode(&mode)),
        targets: InhibitionTargets::from_login1_what(&what),
        can_release: false,
        added_at_unix: now_unix(),
    }
}

/// Pure diff: compare a previous (key -> id) map against a fresh
/// ListInhibitors response. Filters out any entry whose pid equals the
/// daemon's own pid (logind reports our outbound fds too, and they're
/// already tracked via their owning ScreenSaver/Portal record).
pub fn diff(
    previous: &HashMap<Login1Key, u64>,
    current: &[Login1InhibitorEntry],
    own_pid: u32,
) -> (Vec<Login1InhibitorEntry>, Vec<Login1Key>) {
    use std::collections::HashSet;
    let current_keys: HashSet<Login1Key> = current
        .iter()
        .filter(|e| e.5 != own_pid)
        .map(key_of)
        .collect();

    let added: Vec<_> = current
        .iter()
        .filter(|e| e.5 != own_pid && !previous.contains_key(&key_of(e)))
        .cloned()
        .collect();

    let removed: Vec<_> = previous
        .keys()
        .filter(|k| !current_keys.contains(k))
        .cloned()
        .collect();

    (added, removed)
}

/// Long-running polling loop. Polls `Manager.ListInhibitors` every 5s,
/// diffs against the previous snapshot, applies adds/removes to the
/// registry, and fires `on_change` if anything changed.
pub async fn run(
    proxy: Login1ManagerProxy<'static>,
    registry: Arc<Mutex<Registry>>,
    on_change: Arc<dyn Fn() + Send + Sync>,
    cancel: CancellationToken,
) {
    let own_pid = std::process::id();
    let mut tracked: HashMap<Login1Key, u64> = HashMap::new();

    loop {
        let entries = match proxy.list_inhibitors().await {
            Ok(v) => v,
            Err(e) => {
                tracing::debug!(error = ?e, "logind ListInhibitors failed");
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(POLL_INTERVAL) => continue,
                }
            }
        };

        let (added, removed) = diff(&tracked, &entries, own_pid);
        if !added.is_empty() || !removed.is_empty() {
            let mut reg = registry.lock().await;
            for entry in &added {
                let id = reg.mint_id();
                let record = entry_to_record(id, entry);
                reg.insert(record, None);
                tracked.insert(key_of(entry), id);
            }
            for key in &removed {
                if let Some(id) = tracked.remove(key) {
                    reg.release_record(id);
                }
            }
            drop(reg);
            on_change();
        }

        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = tokio::time::sleep(POLL_INTERVAL) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(what: &str, who: &str, why: &str, mode: &str, pid: u32) -> Login1InhibitorEntry {
        (what.into(), who.into(), why.into(), mode.into(), 1000, pid)
    }

    #[test]
    fn diff_detects_additions_excluding_own_pid() {
        let prev = HashMap::new();
        let curr = vec![
            e("idle", "firefox", "video", "block", 1234),
            e("idle:sleep", "Glimpse", "Manual hold", "block", 9999), // own pid
        ];
        let (added, removed) = diff(&prev, &curr, 9999);
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].1, "firefox");
        assert!(removed.is_empty());
    }

    #[test]
    fn diff_detects_removals() {
        let mut prev = HashMap::new();
        prev.insert(Login1Key { pid: 1234, who: "firefox".into(), why: "video".into() }, 1);
        prev.insert(Login1Key { pid: 5678, who: "apt".into(), why: "upgrade".into() }, 2);
        let curr = vec![e("idle", "firefox", "video", "block", 1234)];
        let (added, removed) = diff(&prev, &curr, 9999);
        assert!(added.is_empty());
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].who, "apt");
    }

    #[test]
    fn parse_mode_defaults_to_block() {
        assert!(matches!(parse_mode("block"), Login1Mode::Block));
        assert!(matches!(parse_mode("delay"), Login1Mode::Delay));
        assert!(matches!(parse_mode("block-weak"), Login1Mode::BlockWeak));
        assert!(matches!(parse_mode("anything-else"), Login1Mode::Block));
    }

    #[test]
    fn entry_to_record_carries_pid_uid_and_targets() {
        let entry = e("sleep:shutdown", "apt", "upgrade", "block", 1234);
        let record = entry_to_record(42, &entry);
        assert_eq!(record.id, 42);
        assert_eq!(record.who, "apt");
        assert!(record.targets.suspend);
        assert!(record.targets.shutdown);
        assert!(!record.targets.idle);
        assert!(!record.can_release);
        match record.source.kind {
            glimpse_core::services::idle_inhibitor::SourceKind::Login1 => {
                assert_eq!(record.source.pid, 1234);
                assert_eq!(record.source.uid, 1000);
            }
            _ => panic!("expected Login1 source kind"),
        }
    }
}
