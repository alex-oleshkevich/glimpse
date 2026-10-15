use anyhow::{Context, Result};
use chrono::{Local, NaiveTime, TimeDelta};
use glimpse_dbus::notifications::{NotificationRecord, NotificationUrgency};
use glimpse_dbus::notifications::{Notifications1Proxy, NotificationsView, decode_snapshot};
use std::collections::HashMap;
use zbus::Connection;

use super::{ABSENT, emit, proxy, reason_or_absent, safe, within, yes_no};
use crate::cli::{DndState, NotificationSendArgs};
use crate::render::{Section, Table, styled};

pub const INDEFINITELY: i64 = 0;

pub async fn notifications_list(connection: &Connection, json: bool) -> Result<()> {
    let view = read(connection).await?;

    if json {
        return emit(&view);
    }

    crate::render::print(&list_text(&view))?;
    Ok(())
}

fn list_text(view: &NotificationsView) -> String {
    let status = Section::new("Notifications")
        .with(
            Table::default()
                .with_row(["serving".to_owned(), yes_no(view.serving).to_owned()])
                .with_row([
                    "reason".to_owned(),
                    styled::key(&reason_or_absent(&safe(&view.reason))),
                ])
                .with_row(["do not disturb".to_owned(), dnd(view)])
                .render(),
        )
        .render();
    let store = Section::new("Store")
        .with(
            Table::default()
                .with_headers(["ID", "APP", "URGENCY", "SUMMARY"])
                .with_empty("the store is empty")
                .with_rows(view.notifications.iter().map(row))
                .render(),
        )
        .render();
    format!("{status}\n\n{store}")
}

pub async fn notifications_list_watch(connection: &Connection, json: bool) -> Result<()> {
    super::watch::watch(
        connection,
        glimpse_dbus::notifications::GLIMPSE_NOTIFICATIONS_BUS_NAME,
        glimpse_dbus::notifications::GLIMPSE_NOTIFICATIONS_OBJECT_PATH,
        "me.aresa.Glimpse.Notifications1",
        |owner| async move {
            let proxy = within(
                Notifications1Proxy::builder(connection)
                    .destination(owner)?
                    .cache_properties(zbus::proxy::CacheProperties::No)
                    .build(),
            )
            .await?;
            let view = read_proxy(&proxy).await?;
            if json {
                Ok(serde_json::to_string(&view)?)
            } else {
                Ok(list_text(&view))
            }
        },
    )
    .await
}

pub async fn notifications_send(
    connection: &Connection,
    args: &NotificationSendArgs,
    json: bool,
) -> Result<()> {
    let actions = args
        .action
        .iter()
        .map(|action| {
            action
                .split_once('=')
                .filter(|(key, label)| !key.is_empty() && !label.is_empty())
                .ok_or_else(|| {
                    crate::errors::Refused(format!("invalid action `{action}`: use KEY=LABEL"))
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let actions = actions
        .into_iter()
        .flat_map(|(key, label)| [key, label])
        .collect::<Vec<_>>();
    let mut hints = HashMap::from([("urgency", zbus::zvariant::Value::from(args.urgency.as_u8()))]);
    if let Some(image) = &args.image {
        hints.insert("image-path", zbus::zvariant::Value::from(image.as_str()));
    }
    let id = within(
        proxy::<glimpse_dbus::freedesktop_notifications::FreedesktopNotificationsProxy>(connection)
            .await?
            .notify(
                &args.app_name,
                args.replace,
                &args.icon,
                &args.summary,
                &args.body,
                &actions,
                hints,
                args.expire,
            ),
    )
    .await
    .context("cannot send notification")?;
    if json {
        emit(&serde_json::json!({"id": id}))
    } else {
        crate::render::print(&id.to_string()).map(|_| ())
    }
}

pub async fn notifications_remove(connection: &Connection, id: u32) -> Result<()> {
    within(proxy::<Notifications1Proxy>(connection).await?.remove(id))
        .await
        .with_context(|| format!("cannot remove notification {id}"))
}

pub async fn notifications_activate(connection: &Connection, id: u32) -> Result<()> {
    within(
        proxy::<Notifications1Proxy>(connection)
            .await?
            .activate(id, ""),
    )
    .await
    .with_context(|| format!("cannot activate notification {id}"))
}

pub async fn notifications_action(connection: &Connection, id: u32, key: &str) -> Result<()> {
    within(
        proxy::<Notifications1Proxy>(connection)
            .await?
            .invoke_action(id, key, ""),
    )
    .await
    .with_context(|| format!("cannot invoke action `{key}` on notification {id}"))
}

pub async fn notifications_dismiss(connection: &Connection, id: u32) -> Result<()> {
    within(proxy::<Notifications1Proxy>(connection).await?.dismiss(id))
        .await
        .with_context(|| format!("cannot dismiss notification {id}"))?;
    Ok(())
}

pub async fn notifications_clear(connection: &Connection, app: Option<String>) -> Result<()> {
    let proxy = proxy::<Notifications1Proxy>(connection).await?;
    match app {
        Some(app) => within(proxy.clear_application(&app))
            .await
            .with_context(|| format!("cannot clear notifications from `{app}`"))?,
        None => within(proxy.clear_all())
            .await
            .context("cannot clear the notification store")?,
    }
    Ok(())
}

pub async fn notifications_dnd(
    connection: &Connection,
    state: DndState,
    until: Option<NaiveTime>,
) -> Result<()> {
    let enabled = state == DndState::On;
    let lapses = expiry(enabled, until)?;

    within(
        proxy::<Notifications1Proxy>(connection)
            .await?
            .set_do_not_disturb(enabled, lapses),
    )
    .await
    .context("cannot change do not disturb")?;
    Ok(())
}

pub fn expiry(enabled: bool, until: Option<NaiveTime>) -> Result<i64> {
    let Some(time) = until else {
        return Ok(INDEFINITELY);
    };
    anyhow::ensure!(
        enabled,
        "`--until` needs `dnd on`: there is nothing to lapse when it is off"
    );

    let now = Local::now();
    (0..3)
        .filter_map(|day| {
            (now + TimeDelta::days(day))
                .date_naive()
                .and_time(time)
                .and_local_timezone(Local)
                .earliest()
        })
        .find(|candidate| *candidate > now)
        .map(|candidate| candidate.timestamp_micros())
        .context("that time does not arrive in the next two days in this timezone")
}

async fn read(connection: &Connection) -> Result<NotificationsView> {
    read_proxy(&proxy::<Notifications1Proxy>(connection).await?).await
}

async fn read_proxy(proxy: &Notifications1Proxy<'_>) -> Result<NotificationsView> {
    let snapshot = within(proxy.snapshot())
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
                until.with_timezone(&Local).format("%Y-%m-%d %H:%M")
            ))
        ),
        None => styled::warn("on"),
    }
}

pub fn row(record: &NotificationRecord) -> [String; 4] {
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
