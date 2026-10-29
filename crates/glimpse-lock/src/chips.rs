use chrono::{DateTime, Utc};
use glimpse_dbus::notifications::{NotificationRecord, NotificationUrgency};
use glimpse_widgets::ChipGroup;
use gtk4::gio;
use gtk4::prelude::Cast;

const GENERIC_ICON: &str = "preferences-system-notifications-symbolic";

struct Group {
    app_id: String,
    app: String,
    icon: Option<gio::Icon>,
    count: u32,
    critical: bool,
    latest: DateTime<Utc>,
}

pub fn groups_of(records: &[NotificationRecord], since: DateTime<Utc>) -> Vec<ChipGroup> {
    let mut kept: Vec<&NotificationRecord> = records
        .iter()
        .filter(|record| record.created >= since)
        .collect();
    if kept.is_empty() {
        return Vec::new();
    }
    kept.sort_by_key(|record| std::cmp::Reverse(record.created));

    app_groups(&kept)
}

fn app_groups(kept: &[&NotificationRecord]) -> Vec<ChipGroup> {
    let mut groups: Vec<Group> = Vec::new();
    for record in kept {
        match groups
            .iter_mut()
            .find(|group| group.app_id == record.app_id)
        {
            Some(group) => {
                group.count += 1;
                group.critical |= record.urgency == NotificationUrgency::Critical;
                if group.icon.is_none() {
                    group.icon = themed_icon(record.icon.as_deref());
                }
            }
            None => groups.push(Group {
                app_id: record.app_id.clone(),
                app: record.app_name.clone(),
                icon: themed_icon(record.icon.as_deref()),
                count: 1,
                critical: record.urgency == NotificationUrgency::Critical,
                latest: record.created,
            }),
        }
    }

    groups.sort_by(|a, b| {
        b.critical
            .cmp(&a.critical)
            .then(b.latest.cmp(&a.latest))
            .then(a.app_id.cmp(&b.app_id))
    });

    groups
        .into_iter()
        .map(|group| ChipGroup {
            key: group.app_id,
            app: group.app,
            icon: group.icon.or_else(generic_icon),
            count: group.count,
        })
        .collect()
}

fn themed_icon(name: Option<&str>) -> Option<gio::Icon> {
    name.filter(|name| !name.is_empty())
        .map(|name| gio::ThemedIcon::new(name).upcast())
}

fn generic_icon() -> Option<gio::Icon> {
    Some(gio::ThemedIcon::new(GENERIC_ICON).upcast())
}

#[cfg(test)]
mod tests;
