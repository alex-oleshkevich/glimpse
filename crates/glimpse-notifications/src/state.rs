use std::collections::HashMap;

use glimpse_config::{NotificationEdge, Notifications};
use glimpse_dbus::notifications::NotificationRecord;
use glimpse_services::OutputInfo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    pub connector: Option<String>,
    pub edge: NotificationEdge,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Delta {
    pub appeared: Vec<u32>,
    pub replaced: Vec<u32>,
    pub removed: Vec<u32>,
}

pub struct PopupState {
    settings: Notifications,
    records: HashMap<u32, NotificationRecord>,
    visible: Vec<u32>,
    outputs: Vec<OutputInfo>,
    placement: Option<Placement>,
    awaiting_baseline: bool,
    dnd_known: bool,
    session_known: bool,
    dnd: bool,
    locked: bool,
    private: bool,
}

impl PopupState {
    pub fn new(settings: Notifications) -> Self {
        Self {
            settings,
            records: HashMap::new(),
            visible: Vec::new(),
            outputs: Vec::new(),
            placement: None,
            awaiting_baseline: true,
            dnd_known: false,
            session_known: false,
            dnd: false,
            locked: false,
            private: false,
        }
    }

    pub fn settings(&self) -> &Notifications {
        &self.settings
    }

    pub fn visible(&self) -> &[u32] {
        &self.visible
    }

    pub fn record(&self, id: u32) -> Option<&NotificationRecord> {
        self.records.get(&id)
    }

    pub fn placement(&self) -> Option<&Placement> {
        self.placement.as_ref()
    }

    pub fn disconnected(&mut self) -> Delta {
        self.awaiting_baseline = true;
        self.dnd_known = false;
        self.session_known = false;
        self.clear()
    }

    pub fn configure(&mut self, settings: Notifications) -> Delta {
        let enabled = settings.enabled;
        let cap = settings.max_items as usize;
        self.settings = settings;
        if !enabled {
            return self.clear();
        }
        self.trim(cap)
    }

    pub fn set_dnd(&mut self, dnd: bool) -> Delta {
        self.dnd_known = true;
        self.dnd = dnd;
        if dnd { self.clear() } else { Delta::default() }
    }

    pub fn set_session(&mut self, locked: bool, private: bool) -> Delta {
        self.session_known = true;
        self.locked = locked;
        self.private = private;
        if locked || private {
            self.clear()
        } else {
            Delta::default()
        }
    }

    pub fn set_outputs(&mut self, outputs: Vec<OutputInfo>) -> bool {
        self.outputs = outputs;
        let reselect =
            self.placement
                .as_ref()
                .is_some_and(|placement| match placement.connector.as_deref() {
                    Some(connector) => !self
                        .outputs
                        .iter()
                        .any(|output| output.enabled && output.connector == connector),
                    None => self.outputs.iter().any(|output| output.enabled),
                });
        if reselect {
            self.placement = Some(self.next_placement());
        }
        reselect
    }

    pub fn update(&mut self, records: Vec<NotificationRecord>) -> Delta {
        let next: HashMap<u32, NotificationRecord> = records
            .into_iter()
            .map(|record| (record.id, record))
            .collect();
        if self.awaiting_baseline {
            self.awaiting_baseline = false;
            self.records = next;
            return Delta::default();
        }

        let prior_visible = self.visible.clone();
        let mut removed = Vec::new();
        self.visible.retain(|id| {
            let keep = next.get(id).is_some_and(|record| record.unread);
            if !keep {
                removed.push(*id);
            }
            keep
        });

        let mut appeared = Vec::new();
        let mut replaced = Vec::new();
        if !self.gated() {
            let mut fresh: Vec<&NotificationRecord> = next
                .values()
                .filter(|record| record.unread && !self.records.contains_key(&record.id))
                .collect();
            fresh.sort_by(|left, right| {
                right
                    .created
                    .cmp(&left.created)
                    .then_with(|| right.id.cmp(&left.id))
            });
            for record in fresh.iter().rev() {
                self.visible.insert(0, record.id);
            }
            appeared.extend(fresh.into_iter().map(|record| record.id));

            for id in &self.visible {
                if prior_visible.contains(id)
                    && self
                        .records
                        .get(id)
                        .zip(next.get(id))
                        .is_some_and(|(old, new)| old != new)
                {
                    replaced.push(*id);
                }
            }
        }

        self.records = next;
        let overflow = self.trim(self.settings.max_items as usize);
        removed.extend(overflow.removed);
        appeared.retain(|id| self.visible.contains(id));
        if self.visible.is_empty() {
            self.placement = None;
        } else if self.placement.is_none() {
            self.placement = Some(self.next_placement());
        }
        Delta {
            appeared,
            replaced,
            removed,
        }
    }

    pub fn hide(&mut self, id: u32) -> Delta {
        match self.visible.iter().position(|shown| *shown == id) {
            Some(index) => {
                self.visible.remove(index);
                if self.visible.is_empty() {
                    self.placement = None;
                }
                Delta {
                    removed: vec![id],
                    ..Delta::default()
                }
            }
            None => Delta::default(),
        }
    }

    fn gated(&self) -> bool {
        !self.settings.enabled
            || !self.dnd_known
            || !self.session_known
            || self.dnd
            || self.locked
            || self.private
    }

    fn clear(&mut self) -> Delta {
        let removed = std::mem::take(&mut self.visible);
        self.placement = None;
        Delta {
            removed,
            ..Delta::default()
        }
    }

    fn trim(&mut self, cap: usize) -> Delta {
        let removed = self.visible.split_off(self.visible.len().min(cap));
        if self.visible.is_empty() {
            self.placement = None;
        }
        Delta {
            removed,
            ..Delta::default()
        }
    }

    fn next_placement(&self) -> Placement {
        let connector = self
            .settings
            .monitor
            .as_ref()
            .filter(|target| {
                self.outputs
                    .iter()
                    .any(|output| output.enabled && output.connector == target.as_str())
            })
            .cloned()
            .or_else(|| {
                self.outputs
                    .iter()
                    .find(|output| output.enabled && output.focused)
                    .map(|output| output.connector.clone())
            })
            .or_else(|| {
                self.outputs
                    .iter()
                    .find(|output| output.enabled)
                    .map(|output| output.connector.clone())
            });
        Placement {
            connector,
            edge: self.settings.edge,
        }
    }
}

#[cfg(test)]
mod tests;
