use std::collections::HashMap;

use gettextrs::gettext;
use glimpse_compositors::{Compositor, detect_compositor};
use glimpse_dbus::DEADLINE;
use glimpse_dbus::freedesktop_notifications::FreedesktopNotificationsProxy;
use tokio::sync::mpsc;
use zbus::zvariant::Value;

use crate::lifecycle::{Notice, Reason};

const CRITICAL: u8 = 2;
const NORMAL: u8 = 1;

pub fn text(notice: Notice) -> (String, String) {
    let (summary, reason) = match notice {
        Notice::NotLocked(reason) => (gettext("Screen not locked"), reason),
        Notice::SuspendingUnlocked(reason) => (gettext("Suspending unlocked"), reason),
        Notice::RestoreDisabled => {
            return (
                gettext("Lock restore is off"),
                gettext(
                    "Hyprland will refuse a restarted glimpse-lock. Set misc:allow_session_lock_restore to true.",
                ),
            );
        }
        Notice::PasswordExpired => {
            return (
                gettext("Password expired"),
                gettext("Your password has expired. Change it now with passwd."),
            );
        }
    };
    let body = match reason {
        Reason::CantVerify => {
            gettext("glimpse-lock can't verify passwords in this session. Run glimpse-lock check.")
        }
        Reason::NoSessionLock => gettext("This compositor has no session lock."),
        Reason::LockFailed => gettext("The compositor refused the lock."),
    };
    (summary, body)
}

fn urgency(notice: Notice) -> u8 {
    match notice {
        Notice::NotLocked(_) | Notice::SuspendingUnlocked(_) => CRITICAL,
        Notice::PasswordExpired | Notice::RestoreDisabled => NORMAL,
    }
}

async fn connect() -> Result<FreedesktopNotificationsProxy<'static>, String> {
    let connect = async {
        let connection = zbus::Connection::session().await?;
        FreedesktopNotificationsProxy::new(&connection).await
    };
    match tokio::time::timeout(DEADLINE, connect).await {
        Ok(result) => result.map_err(|error| error.to_string()),
        Err(_) => Err("the session bus did not answer in time".to_owned()),
    }
}

async fn post(proxy: &FreedesktopNotificationsProxy<'_>, notice: Notice) -> Result<u32, String> {
    let (summary, body) = text(notice);
    let hints = HashMap::from([("urgency", Value::U8(urgency(notice)))]);
    let notify = proxy.notify(
        "glimpse-lock",
        0,
        "system-lock-screen-symbolic",
        &summary,
        &body,
        &[],
        hints,
        -1,
    );
    match tokio::time::timeout(DEADLINE, notify).await {
        Ok(result) => result.map_err(|error| error.to_string()),
        Err(_) => Err("the notification daemon did not answer in time".to_owned()),
    }
}

pub async fn restore_notice(compositor: &Compositor) -> Option<Notice> {
    match compositor.allows_session_lock_restore().await {
        Ok(true) => None,
        Ok(false) => Some(Notice::RestoreDisabled),
        Err(error) => {
            tracing::warn!(%error, "could not read the session lock restore option");
            None
        }
    }
}

pub fn spawn() -> mpsc::UnboundedSender<Notice> {
    let (notices, mut receiver) = mpsc::unbounded_channel::<Notice>();
    let restore = notices.clone();
    relm4::spawn(async move {
        if let Some(notice) = restore_notice(&detect_compositor()).await {
            let _ = restore.send(notice);
        }
    });
    relm4::spawn(async move {
        let mut proxy = None;
        while let Some(notice) = receiver.recv().await {
            if proxy.is_none() {
                proxy = connect()
                    .await
                    .inspect_err(
                        |error| tracing::error!(%error, "no session bus for notifications"),
                    )
                    .ok();
            }
            let posted = match &proxy {
                Some(proxy) => post(proxy, notice).await,
                None => Err("no session bus".to_owned()),
            };
            if let Err(error) = posted {
                let (summary, body) = text(notice);
                tracing::error!(%error, summary, body, "could not post a notification");
            }
        }
    });
    notices
}

#[cfg(test)]
mod tests;
