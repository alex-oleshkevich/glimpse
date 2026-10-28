use std::collections::HashMap;

use gettextrs::ngettext;
use glimpse_dbus::DEADLINE;
use glimpse_dbus::freedesktop_notifications::FreedesktopNotificationsProxy;
use glimpse_services::{CalendarState, FailingSource};
use tokio::sync::watch;
use zbus::zvariant::Value;

const APP_NAME: &str = "glimpse-calendar";
const ICON: &str = "x-office-calendar-symbolic";
const URGENCY_NORMAL: u8 = 1;

#[derive(Default)]
struct Shown {
    id: u32,
    sources: Vec<String>,
}

pub(crate) async fn follow(session: zbus::Connection, mut state: watch::Receiver<CalendarState>) {
    let proxy = match FreedesktopNotificationsProxy::new(&session).await {
        Ok(proxy) => proxy,
        Err(error) => {
            tracing::warn!(%error, "calendar failures cannot be notified");
            return;
        }
    };
    let mut shown = Shown::default();
    loop {
        let failing = state.borrow_and_update().failing.clone();
        reconcile(&proxy, &mut shown, &failing).await;
        if state.changed().await.is_err() {
            return;
        }
    }
}

async fn reconcile(
    proxy: &FreedesktopNotificationsProxy<'_>,
    shown: &mut Shown,
    failing: &[FailingSource],
) {
    let ids: Vec<String> = failing.iter().map(|source| source.id.clone()).collect();
    if ids == shown.sources {
        return;
    }
    if failing.is_empty() {
        if shown.id != 0 {
            match tokio::time::timeout(DEADLINE, proxy.close_notification(shown.id)).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => tracing::debug!(%error, "the calendar notification was gone"),
                Err(_) => {
                    tracing::warn!("the notification daemon did not answer in time");
                    return;
                }
            }
        }
        *shown = Shown::default();
        tracing::info!("every calendar source reads again");
        return;
    }
    tracing::warn!(sources = %log_line(failing), "calendar sources cannot be read");
    let (summary, body) = text(failing);
    let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
    hints.insert("urgency", Value::U8(URGENCY_NORMAL));
    let posted = tokio::time::timeout(
        DEADLINE,
        proxy.notify(APP_NAME, shown.id, ICON, &summary, &body, &[], hints, -1),
    )
    .await;
    match posted {
        Ok(Ok(id)) => {
            shown.id = id;
            shown.sources = ids;
        }
        Ok(Err(error)) => tracing::warn!(%error, "cannot post the calendar notification"),
        Err(_) => tracing::warn!("the notification daemon did not answer in time"),
    }
}

fn log_line(failing: &[FailingSource]) -> String {
    failing
        .iter()
        .map(|source| format!("{}: {}", source.id, source.reason))
        .collect::<Vec<_>>()
        .join("; ")
}

fn text(failing: &[FailingSource]) -> (String, String) {
    let count = u32::try_from(failing.len()).unwrap_or(u32::MAX);
    let summary = ngettext(
        "{count} calendar cannot be read",
        "{count} calendars cannot be read",
        count,
    )
    .replace("{count}", &count.to_string());
    let body = failing
        .iter()
        .map(|source| format!("{}: {}", source.name, source.reason))
        .collect::<Vec<_>>()
        .join("\n");
    let escaped = body
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    (summary, glimpse_utils::markup::sanitize_body(&escaped))
}

#[cfg(test)]
mod tests;
