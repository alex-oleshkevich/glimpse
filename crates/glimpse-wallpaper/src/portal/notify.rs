use std::collections::HashMap;
use std::future::Future;
use std::time::Duration;

use futures_util::StreamExt;
use gettextrs::{gettext, ngettext};
use glimpse_dbus::freedesktop_notifications::FreedesktopNotificationsProxy;
use zbus::Connection;
use zbus::zvariant::Value;

pub const UNDO_KEY: &str = "glimpse-wallpaper-undo";
const EXPIRE_MS: i32 = 8_000;
const GRACE: Duration = Duration::from_secs(30);

pub fn changed_summary() -> String {
    gettext("Wallpaper changed")
}

pub fn not_changed_summary() -> String {
    gettext("Wallpaper not changed")
}

pub fn changed_body(outputs_kept: usize) -> String {
    match outputs_kept {
        0 => String::new(),
        count => ngettext(
            "{count} output keeps its own image.",
            "{count} outputs keep their own image.",
            count as u32,
        )
        .replace("{count}", &count.to_string()),
    }
}

pub fn lockscreen_body() -> String {
    gettext("Only the background can be set.")
}

pub fn failed_body() -> String {
    gettext("The wallpaper could not be saved.")
}

fn escaped_body(body: &str) -> String {
    let escaped = body
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    glimpse_utils::markup::sanitize_body(&escaped)
}

async fn within<T>(work: impl Future<Output = Result<T, String>>) -> Result<T, String> {
    match tokio::time::timeout(glimpse_dbus::DEADLINE, work).await {
        Ok(result) => result,
        Err(_) => Err("the notification daemon did not answer in time".to_owned()),
    }
}

async fn proxy(connection: &Connection) -> Result<FreedesktopNotificationsProxy<'static>, String> {
    within(async {
        FreedesktopNotificationsProxy::new(connection)
            .await
            .map_err(|error| error.to_string())
    })
    .await
}

async fn send(
    proxy: &FreedesktopNotificationsProxy<'_>,
    replaces_id: u32,
    summary: &str,
    body: &str,
    actions: &[&str],
) -> Result<u32, String> {
    let name = gettext("Wallpaper");
    let body = escaped_body(body);
    let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
    hints.insert("transient", Value::from(true));
    within(async {
        proxy
            .notify(
                &name,
                replaces_id,
                "",
                summary,
                &body,
                actions,
                hints,
                EXPIRE_MS,
            )
            .await
            .map_err(|error| error.to_string())
    })
    .await
}

pub async fn plain(connection: &Connection, summary: String, body: String) {
    let posted = async {
        let proxy = proxy(connection).await?;
        send(&proxy, 0, &summary, &body, &[]).await
    };
    if let Err(error) = posted.await {
        tracing::warn!(%error, "cannot post the notification");
    }
}

enum Heard {
    Action(String),
    Closed,
}

fn heard(message: &zbus::message::Message, id: u32) -> Option<Heard> {
    let header = message.header();
    let member = header.member()?;
    match member.as_str() {
        "ActionInvoked" => {
            let (got, key) = message.body().deserialize::<(u32, String)>().ok()?;
            (got == id).then_some(Heard::Action(key))
        }
        "NotificationClosed" => {
            let (got, _reason) = message.body().deserialize::<(u32, u32)>().ok()?;
            (got == id).then_some(Heard::Closed)
        }
        _ => None,
    }
}

pub async fn changed(
    connection: &Connection,
    replaces_id: u32,
    body: String,
    on_posted: impl FnOnce(u32),
    on_undo: impl FnOnce() + Send + 'static,
) {
    let run: Result<(), String> = (async {
        let proxy = proxy(connection).await?;
        let mut signals = within(async {
            proxy
                .inner()
                .receive_all_signals()
                .await
                .map(glimpse_dbus::events)
                .map_err(|error| error.to_string())
        })
        .await?;
        let undo = gettext("Undo");
        let actions = [UNDO_KEY, undo.as_str()];
        let id = send(&proxy, replaces_id, &changed_summary(), &body, &actions).await?;
        on_posted(id);
        let mut grace = std::pin::pin!(tokio::time::sleep(GRACE));
        loop {
            tokio::select! {
                biased;
                message = signals.next() => match message {
                    Some(glimpse_dbus::Delivery::Missed) => {
                        tracing::warn!("notification signals were dropped under load");
                    }
                    Some(glimpse_dbus::Delivery::Item(message)) => match heard(&message, id) {
                        Some(Heard::Action(key)) => {
                            if key == UNDO_KEY {
                                on_undo();
                            }
                            return Ok(());
                        }
                        Some(Heard::Closed) => return Ok(()),
                        None => {}
                    },
                    None => return Ok(()),
                },
                _ = &mut grace => return Ok(()),
            }
        }
    })
    .await;
    if let Err(error) = run {
        tracing::warn!(%error, "cannot post the notification");
    }
}
