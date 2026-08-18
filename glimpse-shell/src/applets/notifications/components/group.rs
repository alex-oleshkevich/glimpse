use std::collections::HashMap;

use glimpse_core::services::notifications::model::NotificationEntry;

use super::super::format;

const GROUP_THRESHOLD: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NotificationListItem {
    Group(NotificationGroupModel),
    Notification(NotificationEntry),
}

impl NotificationListItem {
    pub fn timestamp(&self) -> u64 {
        match self {
            Self::Group(group) => group.lead.timestamp,
            Self::Notification(notification) => notification.timestamp,
        }
    }

    pub fn id(&self) -> u32 {
        match self {
            Self::Group(group) => group.lead.id,
            Self::Notification(notification) => notification.id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NotificationGroupModel {
    pub key: String,
    pub app_name: String,
    pub icon: String,
    pub ids: Vec<u32>,
    pub lead: NotificationEntry,
    pub notifications: Vec<NotificationEntry>,
}

pub(crate) fn notification_items(notifications: &[NotificationEntry]) -> Vec<NotificationListItem> {
    let mut items = Vec::new();
    for group in group_notifications(notifications) {
        if group.notifications.len() >= GROUP_THRESHOLD {
            items.push(NotificationListItem::Group(group));
        } else {
            items.extend(
                group
                    .notifications
                    .into_iter()
                    .map(NotificationListItem::Notification),
            );
        }
    }

    items.sort_by(|left, right| {
        right
            .timestamp()
            .cmp(&left.timestamp())
            .then_with(|| right.id().cmp(&left.id()))
    });
    items
}

fn group_notifications(notifications: &[NotificationEntry]) -> Vec<NotificationGroupModel> {
    let mut by_key = HashMap::<String, Vec<NotificationEntry>>::new();
    for notification in notifications {
        by_key
            .entry(notification_group_key(notification))
            .or_default()
            .push(notification.clone());
    }

    let mut groups = by_key
        .into_iter()
        .filter_map(|(key, mut notifications)| {
            notifications.sort_by(|left, right| {
                right
                    .timestamp
                    .cmp(&left.timestamp)
                    .then_with(|| right.id.cmp(&left.id))
            });
            let lead = notifications.first()?.clone();
            let app_name = format::source_name(&lead).to_owned();
            let icon = format::app_icon(&lead).to_owned();
            let ids = notifications
                .iter()
                .map(|notification| notification.id)
                .collect::<Vec<_>>();

            Some(NotificationGroupModel {
                key,
                app_name,
                icon,
                ids,
                lead,
                notifications,
            })
        })
        .collect::<Vec<_>>();

    groups.sort_by(|left, right| {
        right
            .lead
            .timestamp
            .cmp(&left.lead.timestamp)
            .then_with(|| right.lead.id.cmp(&left.lead.id))
            .then_with(|| left.app_name.cmp(&right.app_name))
    });
    groups
}

fn notification_group_key(notification: &NotificationEntry) -> String {
    let identified = notification
        .desktop_entry
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let app_name = notification.app_name.trim();
            (!app_name.is_empty()).then_some(app_name)
        });
    match identified {
        Some(key) => key.to_lowercase(),
        // Neither desktop_entry nor app_name identifies the source: use a
        // per-notification unique key so unrelated unidentified
        // notifications never end up in the same group (and get
        // Shift-clicked away together).
        None => format!("notification-{}", notification.id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notification(id: u32, desktop_entry: Option<&str>, app_name: &str) -> NotificationEntry {
        NotificationEntry {
            id,
            app_name: app_name.into(),
            app_icon: String::new(),
            desktop_entry: desktop_entry.map(str::to_owned),
            summary: String::new(),
            body: String::new(),
            urgency: 1,
            actions: Vec::new(),
            image: None,
            timestamp: id.into(),
            resident: false,
        }
    }

    #[test]
    fn unidentified_notifications_never_share_a_group_key() {
        let a = notification(1, None, "");
        let b = notification(2, None, "");

        assert_ne!(notification_group_key(&a), notification_group_key(&b));
    }

    #[test]
    fn identified_notifications_group_by_desktop_entry_or_app_name() {
        let a = notification(1, Some("Firefox"), "");
        let b = notification(2, Some("firefox"), "");
        assert_eq!(notification_group_key(&a), notification_group_key(&b));

        let c = notification(3, None, "Firefox");
        assert_eq!(notification_group_key(&a), notification_group_key(&c));
    }
}
