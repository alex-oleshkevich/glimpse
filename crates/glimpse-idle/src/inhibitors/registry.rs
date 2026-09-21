use std::collections::HashMap;

use glimpse_dbus::idle::{IdleInhibitorRecord, SourceKind};
use zbus::zvariant::OwnedFd;

pub struct InternalRecord {
    pub record: IdleInhibitorRecord,
    pub logind_fd: Option<OwnedFd>,
}

#[derive(Default)]
pub struct Registry {
    next_id: u64,
    next_cookie: u32,
    version: u64,
    records: HashMap<u64, InternalRecord>,
    cookie_to_id: HashMap<u32, u64>,
    bus_name_to_ids: HashMap<String, Vec<u64>>,
}

impl Registry {
    pub const MAX_INHIBITORS_PER_BUS: usize = 32;
    pub const MAX_INHIBITORS_TOTAL: usize = 128;

    pub fn mint_id(&mut self) -> u64 {
        loop {
            let id = self.next_id;
            self.next_id = self.next_id.wrapping_add(1);
            if id != 0 && !self.records.contains_key(&id) {
                return id;
            }
        }
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn mint_cookie(&mut self) -> u32 {
        loop {
            let cookie = self.next_cookie;
            self.next_cookie = self.next_cookie.wrapping_add(1);
            if cookie != 0 && !self.cookie_to_id.contains_key(&cookie) {
                return cookie;
            }
        }
    }

    pub fn check_capacity(&self, owner: Option<&str>) -> Result<(), String> {
        if self.records.len() >= Self::MAX_INHIBITORS_TOTAL {
            return Err(format!(
                "global inhibitor limit ({}) reached",
                Self::MAX_INHIBITORS_TOTAL
            ));
        }
        if let Some(owner) = owner {
            let count = self
                .records
                .values()
                .filter(|internal| internal.record.owned_by(owner))
                .count();
            if count >= Self::MAX_INHIBITORS_PER_BUS {
                return Err(format!(
                    "per-source inhibitor limit ({}) reached",
                    Self::MAX_INHIBITORS_PER_BUS
                ));
            }
        }
        Ok(())
    }

    pub fn insert(&mut self, record: IdleInhibitorRecord, logind_fd: Option<OwnedFd>) -> u64 {
        let id = record.id;
        debug_assert!(
            !self.records.contains_key(&id),
            "insert() called with an id already present in the registry"
        );
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
            SourceKind::ScreenSaver if record.source.cookie != 0 => {
                self.cookie_to_id.insert(record.source.cookie, id);
            }
            SourceKind::ScreenSaver | SourceKind::Login1 | SourceKind::Portal => {}
        }
        if !record.bus_name.is_empty() {
            self.bus_name_to_ids
                .entry(record.bus_name.clone())
                .or_default()
                .push(id);
        }
        self.records
            .insert(id, InternalRecord { record, logind_fd });
        self.version += 1;
        id
    }

    pub fn release_record(&mut self, id: u64) -> Option<u64> {
        let internal = self.records.remove(&id)?;
        tracing::info!(
            id,
            source = ?internal.record.source.kind,
            who = %internal.record.who,
            why = %internal.record.why,
            bus_name = %internal.record.bus_name,
            had_logind_fd = internal.logind_fd.is_some(),
            "idle inhibitor removed"
        );
        drop(internal.logind_fd);
        match internal.record.source.kind {
            SourceKind::ScreenSaver => {
                self.cookie_to_id.remove(&internal.record.source.cookie);
            }
            SourceKind::Login1 | SourceKind::Portal => {}
        }
        if !internal.record.bus_name.is_empty()
            && let Some(ids) = self.bus_name_to_ids.get_mut(&internal.record.bus_name)
        {
            ids.retain(|x| *x != id);
            if ids.is_empty() {
                self.bus_name_to_ids.remove(&internal.record.bus_name);
            }
        }
        self.version += 1;
        Some(id)
    }

    pub fn release_by_bus_name(&mut self, bus_name: &str) -> Vec<u64> {
        let ids: Vec<u64> = self
            .bus_name_to_ids
            .get(bus_name)
            .cloned()
            .unwrap_or_default();
        ids.into_iter()
            .filter(|id| self.release_record(*id).is_some())
            .collect()
    }

    pub fn lookup_by_cookie(&self, cookie: u32) -> Option<u64> {
        self.cookie_to_id.get(&cookie).copied()
    }

    pub fn record_is_owned_by(&self, id: u64, bus_name: &str) -> bool {
        self.records
            .get(&id)
            .is_some_and(|internal| internal.record.bus_name == bus_name)
    }

    pub fn any_idle_target(&self) -> bool {
        self.records
            .values()
            .any(|internal| internal.record.targets.idle)
    }

    pub fn snapshot(&self) -> Vec<IdleInhibitorRecord> {
        self.records
            .values()
            .map(|internal| internal.record.clone())
            .collect()
    }

    pub fn can_release(&self, id: u64) -> Option<bool> {
        self.records
            .get(&id)
            .map(|internal| internal.record.can_release)
    }

    pub fn set_process_name(&mut self, id: u64, process_name: String) {
        match self.records.get_mut(&id) {
            Some(internal) => {
                internal.record.process_name = process_name;
                self.version += 1;
            }
            None => tracing::debug!(id, "set_process_name for an id no longer in the registry"),
        }
    }

    #[cfg(test)]
    fn ids_for_bus_name(&self, bus_name: &str) -> &[u64] {
        self.bus_name_to_ids
            .get(bus_name)
            .map_or(&[], Vec::as_slice)
    }

    #[cfg(test)]
    fn has_bus_name_entry(&self, bus_name: &str) -> bool {
        self.bus_name_to_ids.contains_key(bus_name)
    }

    #[cfg(test)]
    fn count(&self) -> usize {
        self.records.len()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::os::fd::OwnedFd as StdOwnedFd;

    use glimpse_dbus::idle::{IdleInhibitorSource, InhibitionTargets, Login1Mode};

    use super::*;

    fn test_record(id: u64, bus_name: &str, idle: bool) -> IdleInhibitorRecord {
        IdleInhibitorRecord {
            id,
            who: "who".into(),
            why: "why".into(),
            bus_name: bus_name.into(),
            process_name: String::new(),
            source: IdleInhibitorSource::login1(0, 0, Login1Mode::Block),
            targets: if idle {
                InhibitionTargets::idle_only()
            } else {
                InhibitionTargets::NONE
            },
            can_release: true,
            added_at_unix: 0,
        }
    }

    #[test]
    fn cookie_round_trips_through_insert_and_release() {
        let mut r = Registry::default();
        let id = r.mint_id();
        let cookie = r.mint_cookie();
        let record = IdleInhibitorRecord {
            source: IdleInhibitorSource::screen_saver(cookie),
            ..test_record(id, ":1.1", false)
        };
        r.insert(record, None);
        assert_eq!(r.lookup_by_cookie(cookie), Some(id));
        r.release_record(id);
        assert_eq!(r.lookup_by_cookie(cookie), None);
    }

    #[test]
    fn release_record_removes_from_every_secondary_map() {
        let mut r = Registry::default();
        let id = r.mint_id();
        let cookie = r.mint_cookie();
        let record = IdleInhibitorRecord {
            source: IdleInhibitorSource::screen_saver(cookie),
            ..test_record(id, ":1.1", false)
        };
        r.insert(record, None);
        assert_eq!(r.release_record(id), Some(id));
        assert_eq!(r.lookup_by_cookie(cookie), None);
        assert!(!r.has_bus_name_entry(":1.1"));
        assert_eq!(r.count(), 0);
    }

    #[test]
    fn release_record_of_unknown_id_returns_none() {
        let mut r = Registry::default();
        assert!(r.release_record(999).is_none());
    }

    #[test]
    fn release_record_closes_the_held_logind_fd() {
        let mut r = Registry::default();
        let id = r.mint_id();
        let (mut reader, writer) = std::io::pipe().expect("create a test pipe");
        let write_fd: StdOwnedFd = writer.into();
        r.insert(test_record(id, ":1.1", false), Some(write_fd.into()));

        assert_eq!(r.release_record(id), Some(id));

        let mut buf = [0u8; 1];
        let read = reader
            .read(&mut buf)
            .expect("read from the pipe after release");
        assert_eq!(
            read, 0,
            "EOF only appears once every fd referencing the write end is closed"
        );
    }

    #[test]
    fn bus_name_to_ids_tracks_multiple_records_and_release_by_bus_name_clears_them() {
        let mut r = Registry::default();
        let id1 = r.mint_id();
        let cookie1 = r.mint_cookie();
        r.insert(
            IdleInhibitorRecord {
                source: IdleInhibitorSource::screen_saver(cookie1),
                ..test_record(id1, ":1.5", false)
            },
            None,
        );
        let id2 = r.mint_id();
        let cookie2 = r.mint_cookie();
        r.insert(
            IdleInhibitorRecord {
                source: IdleInhibitorSource::screen_saver(cookie2),
                ..test_record(id2, ":1.5", false)
            },
            None,
        );
        assert_eq!(r.ids_for_bus_name(":1.5"), [id1, id2]);
        assert_eq!(r.count(), 2);

        let released = r.release_by_bus_name(":1.5");
        assert_eq!(released.len(), 2);
        assert!(released.contains(&id1));
        assert!(released.contains(&id2));
        assert_eq!(r.lookup_by_cookie(cookie1), None);
        assert_eq!(r.lookup_by_cookie(cookie2), None);
        assert!(
            !r.has_bus_name_entry(":1.5"),
            "the bus-name key itself must be removed, not left behind as an empty Vec"
        );
        assert_eq!(r.count(), 0);
        assert!(r.release_by_bus_name(":1.5").is_empty());
    }

    #[test]
    fn a_portal_app_id_counts_against_the_per_source_cap_like_a_bus_name() {
        let mut r = Registry::default();
        for _ in 0..Registry::MAX_INHIBITORS_PER_BUS {
            let id = r.mint_id();
            let record = IdleInhibitorRecord {
                source: IdleInhibitorSource::portal(format!("/req/{id}"), "org.hostile.App"),
                bus_name: String::new(),
                ..test_record(id, "", false)
            };
            r.insert(record, None);
        }

        assert!(
            r.check_capacity(Some("org.hostile.App")).is_err(),
            "a portal record carries no bus name, so counting only bus names lets one sandboxed \
             app take every global slot"
        );
        assert!(r.check_capacity(Some("org.polite.App")).is_ok());
    }

    #[test]
    fn check_capacity_enforces_per_bus_limit_independently_per_bus_name() {
        let mut r = Registry::default();
        for _ in 0..Registry::MAX_INHIBITORS_PER_BUS {
            let id = r.mint_id();
            let cookie = r.mint_cookie();
            r.insert(
                IdleInhibitorRecord {
                    source: IdleInhibitorSource::screen_saver(cookie),
                    ..test_record(id, ":1.9", false)
                },
                None,
            );
        }
        assert!(r.check_capacity(Some(":1.9")).is_err());
        assert!(r.check_capacity(Some(":1.10")).is_ok());
        assert!(r.check_capacity(None).is_ok());
    }

    #[test]
    fn check_capacity_bounds_no_bus_name_records_by_the_total_cap() {
        let mut r = Registry::default();
        for _ in 0..Registry::MAX_INHIBITORS_TOTAL {
            let id = r.mint_id();
            r.insert(test_record(id, "", false), None);
        }
        assert!(r.check_capacity(None).is_err());
    }

    #[test]
    fn check_capacity_enforces_the_global_cap_even_when_a_bus_has_room() {
        let mut r = Registry::default();
        for i in 0..Registry::MAX_INHIBITORS_TOTAL {
            let id = r.mint_id();
            let bus_name = format!(":1.{i}");
            r.insert(test_record(id, &bus_name, false), None);
        }
        assert!(r.check_capacity(Some(":1.0")).is_err());
        assert!(r.check_capacity(Some("brand-new-bus-name")).is_err());
    }

    #[test]
    fn any_idle_target_reflects_the_live_record_set() {
        let mut r = Registry::default();
        assert!(!r.any_idle_target());
        let id = r.mint_id();
        r.insert(test_record(id, ":1.1", true), None);
        assert!(r.any_idle_target());
        r.release_record(id);
        assert!(!r.any_idle_target());
    }

    #[test]
    fn any_idle_target_stays_false_for_a_non_idle_targeting_record() {
        let mut r = Registry::default();
        let id = r.mint_id();
        r.insert(test_record(id, ":1.1", false), None);
        assert!(!r.any_idle_target());
    }

    #[test]
    fn set_process_name_updates_the_record_and_is_a_no_op_for_an_unknown_id() {
        let mut r = Registry::default();
        let id = r.mint_id();
        r.insert(test_record(id, ":1.1", false), None);

        r.set_process_name(id, "firefox".to_owned());
        assert_eq!(r.snapshot()[0].process_name, "firefox");

        r.set_process_name(999, "nobody".to_owned());
        assert_eq!(r.snapshot()[0].process_name, "firefox");
    }

    #[test]
    fn version_bumps_only_on_state_changing_operations() {
        let mut r = Registry::default();
        let before = r.version();

        let id = r.mint_id();
        let _cookie = r.mint_cookie();
        assert_eq!(r.version(), before, "minting alone must not bump version");

        r.insert(test_record(id, ":1.1", false), None);
        assert_eq!(r.version(), before + 1);

        let _ = r.check_capacity(Some(":1.1"));
        assert_eq!(
            r.version(),
            before + 1,
            "checks alone must not bump version"
        );

        r.set_process_name(id, "firefox".to_owned());
        assert_eq!(r.version(), before + 2);

        r.set_process_name(999, "nobody".to_owned());
        assert_eq!(
            r.version(),
            before + 2,
            "set_process_name for an unknown id must not bump version"
        );

        r.release_record(id);
        assert_eq!(r.version(), before + 3);

        assert!(r.release_record(id).is_none());
        assert_eq!(
            r.version(),
            before + 3,
            "releasing an already-gone id must not bump version"
        );
    }

    #[test]
    fn mint_id_and_mint_cookie_never_repeat_and_cookie_zero_is_never_issued() {
        let mut r = Registry::default();
        let mut ids = std::collections::HashSet::new();
        let mut cookies = std::collections::HashSet::new();
        for _ in 0..1000 {
            let id = r.mint_id();
            assert_ne!(
                id, 0,
                "an id of 0 reads as a sentinel, not a real inhibitor"
            );
            assert!(ids.insert(id), "duplicate id minted");
            let cookie = r.mint_cookie();
            assert_ne!(cookie, 0);
            assert!(cookies.insert(cookie), "duplicate cookie minted");
        }
    }
}
