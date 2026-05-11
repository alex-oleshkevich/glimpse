use std::collections::HashMap;
use std::os::fd::OwnedFd;

use glimpse_core::services::idle_inhibitor::{
    IdleInhibitorRecord, InhibitionTargets, SourceKind, now_unix,
};

/// Per-record server-side state not serialised over D-Bus.
pub struct InternalRecord {
    pub record: IdleInhibitorRecord,
    /// Outbound logind inhibit fd, held while this record is alive. Closed
    /// automatically when the record is released (via `OwnedFd::drop`).
    pub logind_fd: Option<OwnedFd>,
}

#[derive(Default)]
pub struct Registry {
    next_id: u64,
    next_cookie: u32,
    records: HashMap<u64, InternalRecord>,
    cookie_to_id: HashMap<u32, u64>,
    portal_handle_to_id: HashMap<String, u64>,
    bus_name_to_ids: HashMap<String, Vec<u64>>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReleaseOutcome {
    pub id: u64,
    pub had_logind_fd: bool,
}

impl Registry {
    pub fn new() -> Self {
        Self { next_id: 1, next_cookie: 1, ..Self::default() }
    }

    pub fn mint_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn mint_cookie(&mut self) -> u32 {
        loop {
            let c = self.next_cookie;
            self.next_cookie = self.next_cookie.wrapping_add(1).max(1);
            if c != 0 && !self.cookie_to_id.contains_key(&c) {
                return c;
            }
        }
    }

    pub fn insert(&mut self, record: IdleInhibitorRecord, logind_fd: Option<OwnedFd>) {
        let id = record.id;
        tracing::info!(
            id,
            source = ?record.source.kind,
            who = %record.who,
            why = %record.why,
            bus_name = %record.bus_name,
            has_logind_fd = logind_fd.is_some(),
            "idle inhibitor added"
        );
        match record.source.kind {
            SourceKind::ScreenSaver => {
                self.cookie_to_id.insert(record.source.cookie, id);
            }
            SourceKind::Portal => {
                self.portal_handle_to_id.insert(record.source.request_handle.clone(), id);
            }
            SourceKind::Login1 => {}
        }
        if !record.bus_name.is_empty() {
            self.bus_name_to_ids.entry(record.bus_name.clone()).or_default().push(id);
        }
        self.records.insert(id, InternalRecord { record, logind_fd });
    }

    pub fn get(&self, id: u64) -> Option<&InternalRecord> {
        self.records.get(&id)
    }

    pub fn snapshot(&self) -> Vec<IdleInhibitorRecord> {
        let mut v: Vec<_> = self.records.values().map(|r| r.record.clone()).collect();
        v.sort_by_key(|r| r.id);
        v
    }

    /// Release a record by id. Drops the held logind fd (which closes it,
    /// releasing the corresponding logind inhibit). Removes secondary-map
    /// entries.
    pub fn release_record(&mut self, id: u64) -> Option<ReleaseOutcome> {
        let internal = self.records.remove(&id)?;
        let had_logind_fd = internal.logind_fd.is_some();
        tracing::info!(
            id,
            source = ?internal.record.source.kind,
            who = %internal.record.who,
            why = %internal.record.why,
            bus_name = %internal.record.bus_name,
            had_logind_fd,
            "idle inhibitor removed"
        );
        drop(internal.logind_fd);
        match internal.record.source.kind {
            SourceKind::ScreenSaver => {
                self.cookie_to_id.remove(&internal.record.source.cookie);
            }
            SourceKind::Portal => {
                self.portal_handle_to_id.remove(&internal.record.source.request_handle);
            }
            SourceKind::Login1 => {}
        }
        if !internal.record.bus_name.is_empty() {
            if let Some(ids) = self.bus_name_to_ids.get_mut(&internal.record.bus_name) {
                ids.retain(|x| *x != id);
                if ids.is_empty() {
                    self.bus_name_to_ids.remove(&internal.record.bus_name);
                }
            }
        }
        Some(ReleaseOutcome { id, had_logind_fd })
    }

    /// Release every record owned by a given bus_name. Used by the
    /// NameOwnerChanged listener when callers disconnect. Returns the ids
    /// that were released.
    pub fn release_by_bus_name(&mut self, bus_name: &str) -> Vec<u64> {
        let ids: Vec<u64> = self.bus_name_to_ids.get(bus_name).cloned().unwrap_or_default();
        for id in &ids {
            self.release_record(*id);
        }
        ids
    }

    pub fn lookup_by_cookie(&self, cookie: u32) -> Option<u64> {
        self.cookie_to_id.get(&cookie).copied()
    }

    pub fn lookup_by_portal_handle(&self, handle: &str) -> Option<u64> {
        self.portal_handle_to_id.get(handle).copied()
    }

    pub fn ids_for_bus_name(&self, bus_name: &str) -> Vec<u64> {
        self.bus_name_to_ids.get(bus_name).cloned().unwrap_or_default()
    }

    pub fn any_idle_target(&self) -> bool {
        self.records.values().any(|r| r.record.targets.idle)
    }

    pub fn count(&self) -> usize { self.records.len() }

    /// Mutable access for in-place backfill (used by `process_name`
    /// resolution in the ScreenSaver and portal servers).
    #[allow(dead_code)]
    pub(crate) fn records_mut(&mut self) -> &mut HashMap<u64, InternalRecord> {
        &mut self.records
    }
}

/// Convenience constructor for a ScreenSaver-source record.
pub fn build_screen_saver_record(
    id: u64,
    cookie: u32,
    who: String,
    why: String,
    bus_name: String,
    targets: InhibitionTargets,
) -> IdleInhibitorRecord {
    IdleInhibitorRecord {
        id, who, why, bus_name,
        process_name: String::new(),
        source: glimpse_core::services::idle_inhibitor::IdleInhibitorSource::screen_saver(cookie),
        targets,
        can_release: true,
        added_at_unix: now_unix(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::idle_inhibitor::IdleInhibitorSource;

    fn rec(id: u64, source: IdleInhibitorSource, bus_name: &str) -> IdleInhibitorRecord {
        IdleInhibitorRecord {
            id, who: "x".into(), why: "y".into(), bus_name: bus_name.into(),
            process_name: String::new(), source,
            targets: InhibitionTargets::idle_only(),
            can_release: true, added_at_unix: 0,
        }
    }

    #[test]
    fn cookie_lookup_round_trips() {
        let mut r = Registry::new();
        let id = r.mint_id();
        let cookie = r.mint_cookie();
        r.insert(rec(id, IdleInhibitorSource::screen_saver(cookie), ":1.1"), None);
        assert_eq!(r.lookup_by_cookie(cookie), Some(id));
    }

    #[test]
    fn portal_handle_lookup_round_trips() {
        let mut r = Registry::new();
        let id = r.mint_id();
        r.insert(rec(id, IdleInhibitorSource::portal("org.foo".into(), "/req/1".into()), ":1.2"), None);
        assert_eq!(r.lookup_by_portal_handle("/req/1"), Some(id));
    }

    #[test]
    fn release_removes_from_all_maps() {
        let mut r = Registry::new();
        let id = r.mint_id();
        let cookie = r.mint_cookie();
        r.insert(rec(id, IdleInhibitorSource::screen_saver(cookie), ":1.1"), None);
        let outcome = r.release_record(id).unwrap();
        assert_eq!(outcome.id, id);
        assert!(!outcome.had_logind_fd);
        assert!(r.lookup_by_cookie(cookie).is_none());
        assert!(r.ids_for_bus_name(":1.1").is_empty());
        assert_eq!(r.count(), 0);
    }

    #[test]
    fn release_unknown_id_returns_none() {
        let mut r = Registry::new();
        assert!(r.release_record(999).is_none());
    }

    #[test]
    fn bus_name_to_ids_tracks_multiple_records() {
        let mut r = Registry::new();
        let id1 = r.mint_id();
        let cookie1 = r.mint_cookie();
        let id2 = r.mint_id();
        let cookie2 = r.mint_cookie();
        r.insert(rec(id1, IdleInhibitorSource::screen_saver(cookie1), ":1.7"), None);
        r.insert(rec(id2, IdleInhibitorSource::screen_saver(cookie2), ":1.7"), None);
        assert_eq!(r.ids_for_bus_name(":1.7").len(), 2);
        r.release_record(id1);
        assert_eq!(r.ids_for_bus_name(":1.7"), vec![id2]);
    }

    #[test]
    fn release_by_bus_name_clears_all_records_of_that_name() {
        let mut r = Registry::new();
        let id1 = r.mint_id();
        let cookie1 = r.mint_cookie();
        let id2 = r.mint_id();
        let cookie2 = r.mint_cookie();
        r.insert(rec(id1, IdleInhibitorSource::screen_saver(cookie1), ":1.5"), None);
        r.insert(rec(id2, IdleInhibitorSource::screen_saver(cookie2), ":1.5"), None);
        let released = r.release_by_bus_name(":1.5");
        assert_eq!(released.len(), 2);
        assert_eq!(r.count(), 0);
    }

    #[test]
    fn any_idle_target_reflects_current_records() {
        let mut r = Registry::new();
        assert!(!r.any_idle_target());
        let id = r.mint_id();
        let cookie = r.mint_cookie();
        r.insert(rec(id, IdleInhibitorSource::screen_saver(cookie), ":1.1"), None);
        assert!(r.any_idle_target());
        r.release_record(id);
        assert!(!r.any_idle_target());
    }
}
