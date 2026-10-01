use chrono::{DateTime, Utc};
use gettextrs::{gettext, ngettext};
use glimpse_dbus::idle::{HealthKind, IdleInhibitorRecord, IdleProviderState, SourceKind};
use glimpse_widgets::{InhibitorEntry, InhibitorSource, InhibitorTargets};

pub const ICON_IDLE: &str = "view-conceal-symbolic";
pub const ICON_ACTIVE: &str = "view-reveal-symbolic";
const WHY_CAP: usize = 80;

pub fn icon(active: bool) -> &'static str {
    match active {
        true => ICON_ACTIVE,
        false => ICON_IDLE,
    }
}

pub fn unusable(state: &IdleProviderState) -> Option<String> {
    if !state.available {
        return Some(
            state
                .reason
                .clone()
                .unwrap_or_else(|| gettext("Idle daemon not running")),
        );
    }
    match state.health.wayland.kind {
        HealthKind::Degraded => Some(match state.health.wayland.message.is_empty() {
            true => gettext("The compositor is not reporting idle time"),
            false => state.health.wayland.message.clone(),
        }),
        HealthKind::Ready | HealthKind::Unsupported => None,
    }
}

pub fn awake_until(time: &str) -> String {
    gettext("Awake until {time}").replace("{time}", time)
}

pub fn time_left(minutes: i64) -> String {
    if minutes < 60 {
        return ngettext("{n} minute left", "{n} minutes left", minutes as u32)
            .replace("{n}", &minutes.to_string());
    }
    gettext("{hours} h {minutes} min left")
        .replace("{hours}", &(minutes / 60).to_string())
        .replace("{minutes}", &(minutes % 60).to_string())
}

pub fn holds_idle_or_sleep(record: &IdleInhibitorRecord) -> bool {
    record.targets.idle || record.targets.suspend || record.targets.shutdown
}

pub fn hero_subtitle(
    state: &IdleProviderState,
    manual_hold: &[u64],
    ends: Option<&str>,
    left: Option<&str>,
) -> String {
    if !state.available {
        return gettext("Idle daemon not running");
    }
    if state.health.wayland.kind == HealthKind::Degraded {
        return gettext("The compositor is not reporting idle time");
    }
    if state.inhibitors.is_empty() {
        return gettext("Nothing preventing idle");
    }

    let mine = state
        .inhibitors
        .iter()
        .any(|record| manual_hold.contains(&record.id));
    let others: Vec<&IdleInhibitorRecord> = state
        .inhibitors
        .iter()
        .filter(|record| !manual_hold.contains(&record.id))
        .collect();

    let held = match (ends, left) {
        (Some(time), Some(left)) => format!("{} · {left}", awake_until(time)),
        (Some(time), None) => awake_until(time),
        (None, _) => gettext("Awake until you turn it off"),
    };
    match (mine, others.len()) {
        (true, 0) => held,
        (true, n) => format!("{held} · {}", apps_wording(&others, n)),
        (false, n) => apps_wording(&others, n),
    }
}

fn apps_wording(records: &[&IdleInhibitorRecord], count: usize) -> String {
    let sleepy = records
        .iter()
        .any(|record| record.targets.suspend || record.targets.shutdown);
    let template = match sleepy {
        true => ngettext(
            "{n} app preventing idle or sleep",
            "{n} apps preventing idle or sleep",
            count as u32,
        ),
        false => ngettext(
            "{n} app preventing idle",
            "{n} apps preventing idle",
            count as u32,
        ),
    };
    template.replace("{n}", &count.to_string())
}

pub fn tooltip(
    state: &IdleProviderState,
    manual_hold: &[u64],
    ends: Option<&str>,
    format: Option<&str>,
) -> Option<String> {
    if !state.available {
        return state.reason.clone();
    }
    let status = hero_subtitle(state, manual_hold, ends, None);
    Some(match format {
        None => status,
        Some(format) => crate::applets::tokens::render(format, |token| match token {
            "status" => Some(status.as_str()),
            _ => None,
        }),
    })
}

pub fn manual_hold_ids(records: &[IdleInhibitorRecord]) -> Vec<u64> {
    records
        .iter()
        .filter(|record| is_manual_hold(record))
        .map(|record| record.id)
        .collect()
}

fn is_manual_hold(record: &IdleInhibitorRecord) -> bool {
    record.can_release
        && record.source.kind == SourceKind::Login1
        && record.who == "glimpse-idle"
        && record.why == "Manual hold"
}

pub fn row_label(record: &IdleInhibitorRecord) -> String {
    if !record.process_name.is_empty() {
        return record.process_name.clone();
    }
    if !record.who.is_empty() {
        return super::super::app_label(&record.who);
    }
    if !record.bus_name.is_empty() {
        return record.bus_name.clone();
    }
    gettext("Unknown")
}

fn relative(now: DateTime<Utc>, added_at_unix: u64) -> String {
    let added = DateTime::<Utc>::from_timestamp(added_at_unix as i64, 0).unwrap_or(now);
    let minutes = (now - added).num_minutes().max(0);
    if minutes < 1 {
        return gettext("just now");
    }
    if minutes < 60 {
        return ngettext("{n} minute ago", "{n} minutes ago", minutes as u32)
            .replace("{n}", &minutes.to_string());
    }
    let hours = minutes / 60;
    ngettext("{n} hour ago", "{n} hours ago", hours as u32).replace("{n}", &hours.to_string())
}

fn source_suffix(record: &IdleInhibitorRecord) -> Option<String> {
    match record.source.kind {
        SourceKind::Portal => Some(gettext("(Flatpak via portal)")),
        SourceKind::Login1 => Some(
            gettext("(systemd-inhibit · pid {pid})")
                .replace("{pid}", &record.source.pid.to_string()),
        ),
        SourceKind::ScreenSaver => None,
    }
}

fn row_status(record: &IdleInhibitorRecord, now: DateTime<Utc>) -> String {
    let why = match record.why.is_empty() {
        true => gettext("Preventing idle"),
        false => glimpse_utils::clean(&record.why, WHY_CAP),
    };
    let mut status = format!("{why} · {}", relative(now, record.added_at_unix));
    if let Some(suffix) = source_suffix(record) {
        status = format!("{status} {suffix}");
    }
    status
}

fn row_source(kind: SourceKind) -> InhibitorSource {
    match kind {
        SourceKind::ScreenSaver => InhibitorSource::ScreenSaver,
        SourceKind::Portal => InhibitorSource::Portal,
        SourceKind::Login1 => InhibitorSource::Login1,
    }
}

fn row_targets(record: &IdleInhibitorRecord) -> InhibitorTargets {
    InhibitorTargets {
        idle: record.targets.idle,
        suspend: record.targets.suspend,
        shutdown: record.targets.shutdown,
        lid_switch: record.targets.lid_switch,
        power_key: record.targets.power_key,
        suspend_key: record.targets.suspend_key,
        hibernate_key: record.targets.hibernate_key,
    }
}

pub fn to_inhibitor_entries(
    records: &[IdleInhibitorRecord],
    manual_hold: &[u64],
    now: DateTime<Utc>,
) -> Vec<InhibitorEntry> {
    records
        .iter()
        .map(|record| {
            let mine = manual_hold.contains(&record.id);
            InhibitorEntry {
                id: record.id,
                source: row_source(record.source.kind),
                label: match mine {
                    true => gettext("Glimpse"),
                    false => row_label(record),
                },
                status: match mine {
                    true => format!(
                        "{} · {}",
                        gettext("Manual hold"),
                        relative(now, record.added_at_unix)
                    ),
                    false => row_status(record, now),
                },
                targets: row_targets(record),
                can_release: record.can_release,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
