use std::time::SystemTime;

use glimpse_core::services::idle_inhibitor::{
    BackendHealth, HealthKind, IdleInhibitorRecord, InhibitorsHealth, SourceKind,
};

use crate::services::wayland_idle_inhibit::WaylandHealth;

/// Inputs to the subtitle composer. The applet builds this from its
/// own local state plus the proxied daemon state.
pub struct SubtitleInputs<'a> {
    pub daemon_offline: bool,
    pub wayland: &'a WaylandHealth,
    pub backend: &'a InhibitorsHealth,
    pub records: &'a [IdleInhibitorRecord],
    pub own_unique_name: &'a str,
}

/// Compose the hero subtitle using the strict priority table from the
/// spec. First matching row wins.
pub fn subtitle(inputs: &SubtitleInputs<'_>) -> String {
    if inputs.daemon_offline {
        return "Idle daemon not running".into();
    }
    if let WaylandHealth::Unsupported { message } = inputs.wayland {
        return message.clone();
    }
    if inputs.records.is_empty() {
        if let Some(msg) = degraded_message(&inputs.backend.screen_saver) {
            return msg;
        }
        if let Some(msg) = degraded_message(&inputs.backend.portal) {
            return msg;
        }
        if let Some(msg) = degraded_message(&inputs.backend.login1) {
            return msg;
        }
        return "Nothing is preventing idle".into();
    }

    let manual = inputs.records.iter().any(|r| r.bus_name == inputs.own_unique_name);
    let others: Vec<&IdleInhibitorRecord> = inputs
        .records
        .iter()
        .filter(|r| r.bus_name != inputs.own_unique_name)
        .collect();
    let n = others.len();
    let any_suspend_or_shutdown = others
        .iter()
        .any(|r| r.targets.suspend || r.targets.shutdown);

    match (manual, n, any_suspend_or_shutdown) {
        (true, 0, _) => "Manual hold active".into(),
        (false, 1, false) => "1 app preventing idle".into(),
        (false, _, false) => format!("{n} apps preventing idle"),
        (false, 1, true) => "1 app preventing idle or sleep".into(),
        (false, _, true) => format!("{n} apps preventing idle or sleep"),
        (true, _, false) => format!("Manual hold · {n} apps preventing idle"),
        (true, _, true) => format!("Manual hold · {n} apps preventing idle or sleep"),
    }
}

fn degraded_message(h: &BackendHealth) -> Option<String> {
    matches!(h.kind, HealthKind::Degraded).then(|| h.message.clone())
}

/// Display label for a record row. Prefers process_name; falls back to
/// the caller-supplied `who`; final fallback is the bus name (which
/// won't be empty for ScreenSaver/Portal sources).
pub fn row_label(record: &IdleInhibitorRecord) -> String {
    if !record.process_name.is_empty() {
        return record.process_name.clone();
    }
    if !record.who.is_empty() {
        return record.who.clone();
    }
    record.bus_name.clone()
}

/// Secondary status line for a record row. Renders source-specific
/// detail beneath the primary status. None for plain ScreenSaver rows.
pub fn row_secondary(record: &IdleInhibitorRecord) -> Option<String> {
    match record.source.kind {
        SourceKind::Portal => Some("Flatpak via portal".into()),
        SourceKind::Login1 => Some(format!("systemd-inhibit · pid {}", record.source.pid)),
        SourceKind::ScreenSaver => None,
    }
}

/// Coarse relative time formatter for added_at_unix.
pub fn relative_time(added_at_unix: u64) -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dt = now.saturating_sub(added_at_unix);
    if dt < 5 { "just now".into() }
    else if dt < 60 { format!("{dt} s") }
    else if dt < 3600 { format!("{} min", dt / 60) }
    else { format!("{} h", dt / 3600) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::idle_inhibitor::{IdleInhibitorSource, InhibitionTargets};

    fn rec(bus_name: &str, who: &str, suspend: bool) -> IdleInhibitorRecord {
        let mut t = InhibitionTargets::idle_only();
        t.suspend = suspend;
        IdleInhibitorRecord {
            id: 1, who: who.into(), why: "y".into(), bus_name: bus_name.into(),
            process_name: String::new(),
            source: IdleInhibitorSource::screen_saver(1),
            targets: t,
            can_release: true, added_at_unix: 0,
        }
    }

    fn inputs<'a>(
        backend: &'a InhibitorsHealth,
        records: &'a [IdleInhibitorRecord],
        own: &'a str,
    ) -> SubtitleInputs<'a> {
        SubtitleInputs {
            daemon_offline: false,
            wayland: &WaylandHealth::Ready,
            backend,
            records,
            own_unique_name: own,
        }
    }

    #[test]
    fn empty_state() {
        let backend = InhibitorsHealth::default();
        assert_eq!(subtitle(&inputs(&backend, &[], ":1.7")), "Nothing is preventing idle");
    }

    #[test]
    fn manual_hold_only() {
        let backend = InhibitorsHealth::default();
        let recs = vec![rec(":1.7", "Glimpse", false)];
        assert_eq!(subtitle(&inputs(&backend, &recs, ":1.7")), "Manual hold active");
    }

    #[test]
    fn one_external_idle_only() {
        let backend = InhibitorsHealth::default();
        let recs = vec![rec(":1.99", "Firefox", false)];
        assert_eq!(subtitle(&inputs(&backend, &recs, ":1.7")), "1 app preventing idle");
    }

    #[test]
    fn many_externals_with_suspend() {
        let backend = InhibitorsHealth::default();
        let recs = vec![rec(":1.99", "Firefox", false), rec(":1.100", "apt", true)];
        assert_eq!(subtitle(&inputs(&backend, &recs, ":1.7")), "2 apps preventing idle or sleep");
    }

    #[test]
    fn manual_plus_externals_with_sleep_target() {
        let backend = InhibitorsHealth::default();
        let recs = vec![
            rec(":1.7", "Glimpse", true),
            rec(":1.99", "Firefox", false),
            rec(":1.100", "apt", true),
        ];
        assert_eq!(subtitle(&inputs(&backend, &recs, ":1.7")), "Manual hold · 2 apps preventing idle or sleep");
    }

    #[test]
    fn daemon_offline_overrides_everything() {
        let recs = vec![rec(":1.99", "Firefox", true)];
        let i = SubtitleInputs {
            daemon_offline: true,
            wayland: &WaylandHealth::Ready,
            backend: &InhibitorsHealth::default(),
            records: &recs, own_unique_name: ":1.7",
        };
        assert_eq!(subtitle(&i), "Idle daemon not running");
    }

    #[test]
    fn wayland_unsupported_overrides_active_records() {
        let recs = vec![rec(":1.99", "Firefox", false)];
        let i = SubtitleInputs {
            daemon_offline: false,
            wayland: &WaylandHealth::Unsupported { message: "Not supported on this compositor".into() },
            backend: &InhibitorsHealth::default(),
            records: &recs, own_unique_name: ":1.7",
        };
        assert_eq!(subtitle(&i), "Not supported on this compositor");
    }

    #[test]
    fn screen_saver_degraded_message_overrides_empty_state() {
        let mut backend = InhibitorsHealth::default();
        backend.screen_saver = BackendHealth::degraded("Bus name already owned");
        let i = SubtitleInputs {
            daemon_offline: false,
            wayland: &WaylandHealth::Ready,
            backend: &backend,
            records: &[], own_unique_name: ":1.7",
        };
        assert_eq!(subtitle(&i), "Bus name already owned");
    }

    #[test]
    fn row_label_prefers_process_name_then_who_then_bus_name() {
        let mut r = rec(":1.99", "Firefox", false);
        r.process_name = "firefox-bin".into();
        assert_eq!(row_label(&r), "firefox-bin");
        r.process_name = String::new();
        assert_eq!(row_label(&r), "Firefox");
        r.who = String::new();
        assert_eq!(row_label(&r), ":1.99");
    }

    #[test]
    fn row_secondary_per_source_kind() {
        let mut r = rec(":1.99", "Firefox", false);
        assert!(row_secondary(&r).is_none());

        r.source = IdleInhibitorSource::portal("org.mozilla.firefox".into(), "/req/1".into());
        assert_eq!(row_secondary(&r), Some("Flatpak via portal".into()));

        r.source = IdleInhibitorSource::login1(
            4242, 1000, glimpse_core::services::idle_inhibitor::Login1Mode::Block,
        );
        assert_eq!(row_secondary(&r), Some("systemd-inhibit · pid 4242".into()));
    }
}
