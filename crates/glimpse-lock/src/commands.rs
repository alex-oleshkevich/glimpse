use std::path::Path;
use std::time::Duration;

use anyhow::{Context as _, Result};
use futures_util::{Stream, StreamExt};
use glimpse_dbus::login1::{Login1ManagerProxy, Login1SessionProxy, session_path};
use glimpse_dbus::systemd1::{Systemd1ManagerProxy, Systemd1ServiceProxy};

use crate::errors::{ChecksFailed, NotLocked};
use crate::probe::{self, Check};

const LOCK_WAIT: Duration = Duration::from_secs(5);
const UNIT: &str = "glimpse-lock.service";

pub async fn lock() -> Result<()> {
    let bus = zbus::Connection::system()
        .await
        .context("connect to the system bus")?;
    let path = session_path(&bus).await.map_err(anyhow::Error::msg)?;
    let session = Login1SessionProxy::builder(&bus)
        .path(path)?
        .build()
        .await?;
    let changes = session.receive_locked_hint_changed().await;
    if session.locked_hint().await? {
        return Ok(());
    }
    let id = session.id().await?;
    Login1ManagerProxy::new(&bus)
        .await?
        .lock_session(&id)
        .await
        .context("LockSession")?;
    let hints = changes.then(|change| async move { change.get().await.unwrap_or(false) });
    if turns_true(hints, LOCK_WAIT).await {
        Ok(())
    } else {
        Err(NotLocked(LOCK_WAIT).into())
    }
}

pub async fn turns_true(hints: impl Stream<Item = bool>, cap: Duration) -> bool {
    let mut hints = std::pin::pin!(hints);
    tokio::time::timeout(cap, async {
        while let Some(hint) = hints.next().await {
            if hint {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false)
}

pub async fn check(pam_service: &str) -> Result<()> {
    let mut checks = service_checks().await;
    checks.extend(probe::startup(pam_service));
    checks.push(Check {
        name: "user".to_owned(),
        outcome: crate::user::current().map(Some),
    });
    for check in &checks {
        println!("{}", check.line());
    }
    match checks.iter().filter(|check| !check.passed()).count() {
        0 => Ok(()),
        failed => Err(ChecksFailed(failed).into()),
    }
}

async fn service_checks() -> Vec<Check> {
    match main_pid().await {
        Ok(Some(pid)) => probe::process(&Path::new("/proc").join(pid.to_string()), "service "),
        Ok(None) => vec![Check {
            name: "service".to_owned(),
            outcome: Err(format!("{UNIT} is not running")),
        }],
        Err(error) => vec![Check {
            name: "service".to_owned(),
            outcome: Err(format!("{error:#}")),
        }],
    }
}

async fn main_pid() -> Result<Option<u32>> {
    let bus = zbus::Connection::session()
        .await
        .context("connect to the session bus")?;
    let unit = Systemd1ManagerProxy::new(&bus)
        .await?
        .get_unit(UNIT)
        .await
        .with_context(|| format!("{UNIT} is not loaded"))?;
    let pid = Systemd1ServiceProxy::builder(&bus)
        .path(unit)?
        .build()
        .await?
        .main_pid()
        .await?;
    Ok((pid != 0).then_some(pid))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;

    #[tokio::test]
    async fn the_lock_waits_for_the_hint_to_turn_true() {
        let cap = Duration::from_millis(200);
        assert!(turns_true(stream::iter([false, true]), cap).await);
        assert!(!turns_true(stream::iter([false, false]), cap).await);
        assert!(!turns_true(stream::pending(), Duration::from_millis(20)).await);
    }
}
