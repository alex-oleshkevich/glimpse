use anyhow::{Context, Result};
use chrono::Local;
use glimpse_contracts::{NotificationRecord, NotificationUrgency};
use glimpse_dbus::notifications::{Notifications1Proxy, NotificationsView, decode_snapshot};
use zbus::Connection;

use super::{ABSENT, emit, proxy, reason_or_absent, safe, yes_no};
use crate::cli::DndState;
use crate::render::{Section, Table, styled};

const INDEFINITELY: i64 = 0;

pub async fn notifications_list(connection: &Connection, json: bool) -> Result<()> {
    let view = read(connection).await?;

    if json {
        return emit(&view);
    }

    Section::new("Notifications")
        .with(
            Table::new()
                .with_row(["serving".to_owned(), yes_no(view.serving).to_owned()])
                .with_row([
                    "reason".to_owned(),
                    styled::key(&reason_or_absent(&safe(&view.reason))),
                ])
                .with_row(["do not disturb".to_owned(), dnd(&view)])
                .render(),
        )
        .print()?;

    crate::render::print("")?;
    Section::new("Store")
        .with(
            Table::new()
                .with_headers(["ID", "APP", "URGENCY", "SUMMARY"])
                .with_empty("the store is empty")
                .with_rows(view.notifications.iter().map(row))
                .render(),
        )
        .print()
}

pub async fn notifications_dismiss(connection: &Connection, id: u32) -> Result<()> {
    proxy::<Notifications1Proxy>(connection)
        .await?
        .dismiss(id)
        .await
        .with_context(|| format!("cannot dismiss notification {id}"))?;
    Ok(())
}

pub async fn notifications_clear(connection: &Connection, app: Option<String>) -> Result<()> {
    let proxy = proxy::<Notifications1Proxy>(connection).await?;
    match app {
        Some(app) => proxy
            .clear_application(&app)
            .await
            .with_context(|| format!("cannot clear notifications from `{app}`"))?,
        None => proxy
            .clear_all()
            .await
            .context("cannot clear the notification store")?,
    }
    Ok(())
}

pub async fn notifications_dnd(connection: &Connection, state: DndState) -> Result<()> {
    proxy::<Notifications1Proxy>(connection)
        .await?
        .set_do_not_disturb(state == DndState::On, INDEFINITELY)
        .await
        .context("cannot change do not disturb")?;
    Ok(())
}

async fn read(connection: &Connection) -> Result<NotificationsView> {
    let snapshot = proxy::<Notifications1Proxy>(connection)
        .await?
        .snapshot()
        .await
        .context("cannot read the notification store")?;
    decode_snapshot(snapshot)
        .map_err(anyhow::Error::msg)
        .context("the notification provider sent a snapshot this build cannot read")
}

fn dnd(view: &NotificationsView) -> String {
    if !view.do_not_disturb.enabled {
        return "off".to_owned();
    }
    match view.do_not_disturb.until {
        Some(until) => format!(
            "{}  {}",
            styled::warn("on"),
            styled::key(&format!(
                "until {}",
                until.with_timezone(&Local).to_rfc3339()
            ))
        ),
        None => styled::warn("on"),
    }
}

fn row(record: &NotificationRecord) -> [String; 4] {
    [
        record.id.to_string(),
        match record.app_name.is_empty() {
            true => record.app_id.clone(),
            false => record.app_name.clone(),
        },
        urgency(record.urgency),
        match record.unread {
            true => record.summary.clone(),
            false => styled::key(&record.summary),
        },
    ]
}

fn urgency(urgency: NotificationUrgency) -> String {
    match urgency {
        NotificationUrgency::Low => styled::key("low"),
        NotificationUrgency::Normal => "normal".to_owned(),
        NotificationUrgency::Critical => styled::bad("critical"),
        NotificationUrgency::Unknown => ABSENT.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone as _, Utc};

    fn at(hour: u32, minute: u32) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 14, hour, minute, 0)
            .single()
            .expect("a valid instant")
    }

    #[test]
    fn an_unread_notification_is_not_dimmed_and_a_read_one_is() {
        let mut record = NotificationRecord {
            id: 7,
            app_id: "org.example.App".to_owned(),
            app_name: String::new(),
            app_pid: None,
            summary: "Build finished".to_owned(),
            body: None,
            icon: None,
            image: None,
            urgency: NotificationUrgency::Normal,
            actions: Vec::new(),
            progress: None,
            created: at(20, 0),
            unread: true,
            resident: false,
        };

        let [id, app, _, summary] = row(&record);
        assert_eq!(id, "7");
        assert_eq!(
            app, "org.example.App",
            "a nameless sender falls back to its id"
        );
        assert_eq!(summary, "Build finished");

        record.unread = false;
        let [_, _, _, dimmed] = row(&record);
        assert!(dimmed.contains("Build finished"));
        assert_ne!(dimmed, "Build finished", "a read summary is styled");
    }
}
