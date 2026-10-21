use std::collections::{BTreeMap, HashMap};

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
    hold_expiries: BTreeMap<u64, u64>,
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
                .filter(|internal| {
                    if owner.is_empty() {
                        internal.record.source.kind == SourceKind::Portal
                            && internal.record.source.app_id.is_empty()
                    } else {
                        internal.record.owned_by(owner)
                    }
                })
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

    pub fn attach_fd(&mut self, id: u64, logind_fd: OwnedFd) -> bool {
        match self.records.get_mut(&id) {
            Some(internal) => {
                internal.logind_fd = Some(logind_fd);
                true
            }
            None => false,
        }
    }

    pub fn release_record(&mut self, id: u64) -> Option<u64> {
        let internal = self.records.remove(&id)?;
        self.hold_expiries.remove(&id);
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

    pub fn bus_names(&self) -> Vec<String> {
        self.bus_name_to_ids.keys().cloned().collect()
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
        let mut records: Vec<_> = self
            .records
            .values()
            .map(|internal| internal.record.clone())
            .collect();
        records.sort_unstable_by_key(|record| record.id);
        records
    }

    pub fn set_hold_expiry(&mut self, id: u64, expires_at_unix: u64) {
        self.hold_expiries.insert(id, expires_at_unix);
        self.version += 1;
    }

    pub fn hold_expiries(&self) -> BTreeMap<u64, u64> {
        self.hold_expiries.clone()
    }

    pub fn portal_handle(&self, id: u64) -> Option<String> {
        self.records
            .get(&id)
            .filter(|internal| internal.record.source.kind == SourceKind::Portal)
            .map(|internal| internal.record.source.request_handle.clone())
    }

    pub fn portal_handles_of(&self, bus_name: &str) -> Vec<String> {
        self.bus_name_to_ids
            .get(bus_name)
            .into_iter()
            .flatten()
            .filter_map(|id| self.portal_handle(*id))
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
}

#[cfg(test)]
mod tests;
