use chrono::{DateTime, TimeDelta, Utc};
use gettextrs::{gettext, ngettext};
use glimpse_config::NotificationIndicatorStyle;
use glimpse_dbus::notifications::DEFAULT_ACTION;
use glimpse_dbus::notifications::{NotificationRecord, NotificationUrgency};
use glimpse_widgets::{Action, Body, Group, Notification, Severity, Urgency};

pub const BELL: &str = "preferences-system-notifications-symbolic";
pub const MUTED: &str = "notifications-disabled-symbolic";

const MINUTE: i64 = 60;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;

pub struct Chip {
    pub icon: &'static str,
    pub badge: Option<String>,
    pub attention: bool,
    pub severity: Option<Severity>,
}

pub fn unread(records: &[NotificationRecord]) -> usize {
    records.iter().filter(|record| record.unread).count()
}

pub fn critical(records: &[NotificationRecord]) -> bool {
    records
        .iter()
        .any(|record| record.unread && matches!(record.urgency, NotificationUrgency::Critical))
}

pub fn chip(records: &[NotificationRecord], dnd: bool, style: NotificationIndicatorStyle) -> Chip {
    let unread = unread(records);
    Chip {
        icon: if dnd { MUTED } else { BELL },
        badge: (unread > 0 && !dnd && style == NotificationIndicatorStyle::IconCounter)
            .then(|| unread.to_string()),
        attention: unread > 0 && !dnd && style == NotificationIndicatorStyle::IconDot,
        severity: match (dnd, critical(records), style) {
            (true, _, _) => Some(Severity::Info),
            (false, true, NotificationIndicatorStyle::IconDot) => Some(Severity::Error),
            _ => None,
        },
    }
}

pub fn tooltip(records: &[NotificationRecord], dnd: bool, format: Option<&str>) -> String {
    let unread = unread(records);
    if let Some(format) = format {
        return format.replace("{count}", &unread.to_string());
    }
    match (dnd, unread) {
        (true, 0) => gettext("Do not disturb"),
        (true, n) => ngettext(
            "{count} waiting, do not disturb is on",
            "{count} waiting, do not disturb is on",
            n as u32,
        )
        .replace("{count}", &n.to_string()),
        (false, 0) => gettext("No new notifications"),
        (false, n) => ngettext(
            "{count} new notification",
            "{count} new notifications",
            n as u32,
        )
        .replace("{count}", &n.to_string()),
    }
}

pub fn when(now: DateTime<Utc>, created: DateTime<Utc>) -> String {
    let age = now.signed_duration_since(created).max(TimeDelta::zero());
    let seconds = age.num_seconds();
    if seconds < MINUTE {
        return gettext("now");
    }
    if seconds < HOUR {
        return gettext("{count}m").replace("{count}", &(seconds / MINUTE).to_string());
    }
    if seconds < DAY {
        return gettext("{count}h").replace("{count}", &(seconds / HOUR).to_string());
    }
    gettext("{count}d").replace("{count}", &(seconds / DAY).to_string())
}

pub fn groups(records: &[NotificationRecord], now: DateTime<Utc>) -> Vec<Group> {
    let mut groups = Vec::<Group>::new();
    for record in records {
        let note = notification(record, now);
        if let Some(group) = groups.iter_mut().find(|group| group.key == record.app_id) {
            group.notifications.push(note);
        } else {
            groups.push(Group {
                key: record.app_id.clone(),
                app_name: record.app_name.clone(),
                notifications: vec![note],
            });
        }
    }
    groups
}

pub fn id_of(key: &str) -> Option<u32> {
    key.parse().ok()
}

fn notification(record: &NotificationRecord, now: DateTime<Utc>) -> Notification {
    from_record(record, when(now, record.created))
}

fn from_record(record: &NotificationRecord, when: String) -> Notification {
    let active = record.unread;
    Notification {
        key: record.id.to_string(),
        app_name: record.app_name.clone(),
        summary: record.summary.clone(),
        body: record.body.clone().map(Body::Markup),
        when,
        urgency: match record.urgency {
            NotificationUrgency::Critical => Urgency::Critical,
            _ => Urgency::Normal,
        },
        actions: if active {
            record
                .actions
                .iter()
                .filter(|action| action.key != DEFAULT_ACTION)
                .map(|action| Action {
                    key: action.key.clone(),
                    label: action.label.clone(),
                })
                .collect()
        } else {
            Vec::new()
        },
        progress: record.progress,
        unread: active,
        activatable: active,
        ..Notification::default()
    }
}

#[cfg(test)]
mod tests;
