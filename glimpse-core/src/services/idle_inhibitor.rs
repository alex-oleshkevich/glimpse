#![allow(dead_code)]

use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use zbus::zvariant::{OwnedValue, Type, Value};

use crate::services::framework::ServiceHandle;

/// xdg-desktop-portal Inhibit flag bits — see
/// org.freedesktop.portal.Inhibit specification.
pub const PORTAL_FLAG_LOGOUT: u32 = 1;
pub const PORTAL_FLAG_USER_SWITCH: u32 = 2;
pub const PORTAL_FLAG_SUSPEND: u32 = 4;
pub const PORTAL_FLAG_IDLE: u32 = 8;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type, Value, OwnedValue)]
pub struct InhibitionTargets {
    pub idle: bool,
    pub suspend: bool,
    pub shutdown: bool,
    pub lid_switch: bool,
    pub power_key: bool,
    pub suspend_key: bool,
    pub hibernate_key: bool,
}

impl InhibitionTargets {
    pub fn idle_only() -> Self {
        Self { idle: true, ..Self::default() }
    }

    pub fn manual_hold() -> Self {
        Self { idle: true, suspend: true, ..Self::default() }
    }

    /// Parse the colon-separated `what` field from logind. Unknown tokens are ignored.
    pub fn from_login1_what(what: &str) -> Self {
        let mut t = Self::default();
        for token in what.split(':').filter(|s| !s.is_empty()) {
            match token {
                "idle" => t.idle = true,
                "sleep" => t.suspend = true,
                "shutdown" => t.shutdown = true,
                "handle-lid-switch" => t.lid_switch = true,
                "handle-power-key" => t.power_key = true,
                "handle-suspend-key" => t.suspend_key = true,
                "handle-hibernate-key" => t.hibernate_key = true,
                _ => {}
            }
        }
        t
    }

    /// Decode xdg-desktop-portal Inhibit flags.
    pub fn from_portal_flags(flags: u32) -> Self {
        Self {
            idle: flags & PORTAL_FLAG_IDLE != 0,
            suspend: flags & PORTAL_FLAG_SUSPEND != 0,
            shutdown: flags & PORTAL_FLAG_LOGOUT != 0,
            ..Self::default()
        }
    }

    pub fn any(&self) -> bool {
        self.idle || self.suspend || self.shutdown || self.lid_switch
            || self.power_key || self.suspend_key || self.hibernate_key
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type, Value, OwnedValue)]
pub enum Login1Mode {
    #[default]
    Block,
    Delay,
    BlockWeak,
}

/// Tagged-struct shape (was an enum with heterogeneous variants — reshaped because
/// zvariant 5 cannot derive `Type` on that pattern). Use the constructors below
/// rather than building this struct field-by-field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Value, OwnedValue)]
pub enum SourceKind {
    ScreenSaver,
    Portal,
    Login1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Value, OwnedValue)]
pub struct IdleInhibitorSource {
    pub kind: SourceKind,
    /// ScreenSaver only; 0 for other kinds.
    pub cookie: u32,
    /// Portal only; empty for other kinds.
    pub app_id: String,
    /// Portal only; empty for other kinds.
    pub request_handle: String,
    /// Login1 only; 0 for other kinds.
    pub pid: u32,
    /// Login1 only; 0 for other kinds.
    pub uid: u32,
    /// Login1 only; `Block` for other kinds.
    pub mode: Login1Mode,
}

impl IdleInhibitorSource {
    pub fn screen_saver(cookie: u32) -> Self {
        Self {
            kind: SourceKind::ScreenSaver,
            cookie,
            app_id: String::new(),
            request_handle: String::new(),
            pid: 0,
            uid: 0,
            mode: Login1Mode::Block,
        }
    }

    pub fn portal(app_id: String, request_handle: String) -> Self {
        Self {
            kind: SourceKind::Portal,
            cookie: 0,
            app_id,
            request_handle,
            pid: 0,
            uid: 0,
            mode: Login1Mode::Block,
        }
    }

    pub fn login1(pid: u32, uid: u32, mode: Login1Mode) -> Self {
        Self {
            kind: SourceKind::Login1,
            cookie: 0,
            app_id: String::new(),
            request_handle: String::new(),
            pid,
            uid,
            mode,
        }
    }

    /// Returns true if non-applicable fields for this `kind` are at their
    /// zero/empty defaults. Use with `debug_assert!` at construction sites
    /// in callers that build records by hand.
    pub fn is_consistent(&self) -> bool {
        match self.kind {
            SourceKind::ScreenSaver => {
                self.app_id.is_empty() && self.request_handle.is_empty()
                    && self.pid == 0 && self.uid == 0
            }
            SourceKind::Portal => {
                self.cookie == 0 && self.pid == 0 && self.uid == 0
            }
            SourceKind::Login1 => {
                self.cookie == 0 && self.app_id.is_empty() && self.request_handle.is_empty()
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type, Value, OwnedValue)]
pub enum HealthKind {
    #[default]
    Ready,
    Degraded,
    Unsupported,
}

/// Tagged-struct shape (was an enum with per-variant fields — reshaped for the same
/// reason as `IdleInhibitorSource`). `message` is empty for `Ready`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type, Value, OwnedValue)]
pub struct BackendHealth {
    pub kind: HealthKind,
    pub message: String,
}

impl BackendHealth {
    pub fn ready() -> Self { Self::default() }

    pub fn degraded(message: impl Into<String>) -> Self {
        Self { kind: HealthKind::Degraded, message: message.into() }
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self { kind: HealthKind::Unsupported, message: message.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Value, OwnedValue)]
pub struct IdleInhibitorRecord {
    pub id: u64,
    pub who: String,
    pub why: String,
    pub bus_name: String,
    /// Empty string means "not resolved" (was `Option<String>` — reshaped because
    /// `Option<T>` requires the gvariant feature for `Type` derivation, which we
    /// don't enable in this workspace).
    pub process_name: String,
    pub source: IdleInhibitorSource,
    pub targets: InhibitionTargets,
    pub can_release: bool,
    pub added_at_unix: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type, Value, OwnedValue)]
pub struct InhibitorsHealth {
    pub screen_saver: BackendHealth,
    pub portal: BackendHealth,
    pub login1: BackendHealth,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub health: InhibitorsHealth,
    pub inhibitors: Vec<IdleInhibitorRecord>,
}

#[derive(Debug, Clone)]
pub enum Command {
    SetManualHold(bool),
    Release { id: u64 },
}

pub type IdleInhibitorHandle = ServiceHandle<State, Command>;

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login1_what_round_trips_every_token() {
        let all = "shutdown:sleep:idle:handle-power-key:handle-suspend-key:handle-hibernate-key:handle-lid-switch";
        let t = InhibitionTargets::from_login1_what(all);
        assert!(t.idle && t.suspend && t.shutdown && t.lid_switch
            && t.power_key && t.suspend_key && t.hibernate_key);
    }

    #[test]
    fn login1_what_ignores_unknown_tokens() {
        let t = InhibitionTargets::from_login1_what("idle:bogus-token");
        assert!(t.idle);
        assert!(!t.suspend);
    }

    #[test]
    fn portal_flags_decode_correctly() {
        let t = InhibitionTargets::from_portal_flags(0b1101);
        assert!(t.idle);
        assert!(t.suspend);
        assert!(t.shutdown);
        assert!(!t.lid_switch);
    }

    #[test]
    fn manual_hold_targets_idle_and_suspend_only() {
        let t = InhibitionTargets::manual_hold();
        assert!(t.idle && t.suspend);
        assert!(!t.shutdown && !t.power_key && !t.suspend_key && !t.hibernate_key && !t.lid_switch);
    }

    #[test]
    fn source_constructors_populate_only_relevant_fields() {
        let ss = IdleInhibitorSource::screen_saver(42);
        assert!(matches!(ss.kind, SourceKind::ScreenSaver));
        assert_eq!(ss.cookie, 42);
        assert!(ss.app_id.is_empty() && ss.request_handle.is_empty());
        assert_eq!(ss.pid, 0);
        assert!(ss.is_consistent());

        let portal = IdleInhibitorSource::portal("org.foo".into(), "/r/1".into());
        assert!(matches!(portal.kind, SourceKind::Portal));
        assert_eq!(portal.app_id, "org.foo");
        assert_eq!(portal.cookie, 0);
        assert!(portal.is_consistent());

        let l1 = IdleInhibitorSource::login1(123, 1000, Login1Mode::Delay);
        assert!(matches!(l1.kind, SourceKind::Login1));
        assert_eq!(l1.pid, 123);
        assert!(matches!(l1.mode, Login1Mode::Delay));
        assert!(l1.is_consistent());
    }

    #[test]
    fn is_consistent_detects_manually_built_inconsistencies() {
        let mut bad = IdleInhibitorSource::screen_saver(1);
        bad.app_id = "leaked".into();
        assert!(!bad.is_consistent());

        let mut bad2 = IdleInhibitorSource::portal("x".into(), "/y".into());
        bad2.cookie = 5;
        assert!(!bad2.is_consistent());
    }

    #[test]
    fn portal_flag_constants_match_spec() {
        assert_eq!(PORTAL_FLAG_LOGOUT, 1);
        assert_eq!(PORTAL_FLAG_USER_SWITCH, 2);
        assert_eq!(PORTAL_FLAG_SUSPEND, 4);
        assert_eq!(PORTAL_FLAG_IDLE, 8);
    }

    #[test]
    fn backend_health_constructors() {
        assert!(matches!(BackendHealth::ready().kind, HealthKind::Ready));
        assert!(BackendHealth::ready().message.is_empty());
        let d = BackendHealth::degraded("x");
        assert!(matches!(d.kind, HealthKind::Degraded));
        assert_eq!(d.message, "x");
    }

    #[test]
    fn record_serde_round_trip() {
        let r = IdleInhibitorRecord {
            id: 42,
            who: "Firefox".into(),
            why: "Playing video".into(),
            bus_name: ":1.234".into(),
            process_name: "firefox-bin".into(),
            source: IdleInhibitorSource::screen_saver(99),
            targets: InhibitionTargets::idle_only(),
            can_release: true,
            added_at_unix: 1_700_000_000,
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: IdleInhibitorRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(r, back);
    }
}
