use std::time::Duration;

use anyhow::Result;
use futures_util::StreamExt;
use glimpse_dbus::Delivery;
use glimpse_dbus::login1::{Login1SessionProxy, session_path};
use relm4::Sender;

use crate::ask::Input;

const CALL_DEADLINE: Duration = Duration::from_secs(5);
const RETRY_MIN: Duration = Duration::from_secs(1);
const RETRY_MAX: Duration = Duration::from_secs(30);

enum Control {
    Stop,
    Retry,
}

pub async fn follow(system: zbus::Connection, input: Sender<Input>) {
    let mut session = match open(&system).await {
        Ok(session) => session,
        Err(error) => {
            tracing::warn!("logind is unreachable; the session counts as unlocked: {error:#}");
            return;
        }
    };
    let mut delay = RETRY_MIN;
    loop {
        match watch(&session, &input).await {
            Control::Stop => return,
            Control::Retry => {}
        }
        loop {
            tokio::time::sleep(delay).await;
            delay = (delay * 2).min(RETRY_MAX);
            match open(&system).await {
                Ok(next) => {
                    session = next;
                    delay = RETRY_MIN;
                    break;
                }
                Err(error) => {
                    tracing::warn!("logind session lost; retrying: {error:#}");
                }
            }
        }
    }
}

async fn open(system: &zbus::Connection) -> Result<Login1SessionProxy<'static>> {
    match tokio::time::timeout(CALL_DEADLINE, session(system)).await {
        Ok(session) => session,
        Err(_) => Err(anyhow::anyhow!("logind did not answer in time")),
    }
}

async fn watch(session: &Login1SessionProxy<'static>, input: &Sender<Input>) -> Control {
    let changes = match tokio::time::timeout(
        CALL_DEADLINE,
        session
            .inner()
            .receive_property_changed::<bool>("LockedHint"),
    )
    .await
    {
        Ok(changes) => changes,
        Err(_) => return Control::Retry,
    };
    let mut owners = match tokio::time::timeout(CALL_DEADLINE, async {
        session
            .inner()
            .receive_owner_changed()
            .await
            .map(glimpse_dbus::events)
    })
    .await
    {
        Ok(Ok(owners)) => owners,
        _ => return Control::Retry,
    };
    let mut deliveries = glimpse_dbus::events(changes.map(|_| ()));
    let mut locked = None;
    if publish(session, input, &mut locked).await {
        return Control::Stop;
    }
    loop {
        tokio::select! {
            delivery = deliveries.next() => {
                match delivery {
                    None => return Control::Retry,
                    Some(Delivery::Item(()) | Delivery::Missed) => {
                        if publish(session, input, &mut locked).await {
                            return Control::Stop;
                        }
                    }
                }
            }
            owner = owners.next() => {
                match owner {
                    Some(Delivery::Item(Some(_))) => {}
                    _ => return Control::Retry,
                }
            }
        }
    }
}

async fn publish(
    session: &Login1SessionProxy<'static>,
    input: &Sender<Input>,
    locked: &mut Option<bool>,
) -> bool {
    let Some(hint) = read_hint(session).await else {
        return false;
    };
    if *locked == Some(hint) {
        return false;
    }
    *locked = Some(hint);
    input.send(Input::Locked(hint)).is_err()
}

async fn read_hint(session: &Login1SessionProxy<'static>) -> Option<bool> {
    match tokio::time::timeout(CALL_DEADLINE, session.locked_hint()).await {
        Ok(Ok(hint)) => Some(hint),
        _ => None,
    }
}

pub async fn session(system: &zbus::Connection) -> Result<Login1SessionProxy<'static>> {
    let path = session_path(system).await.map_err(anyhow::Error::msg)?;
    Ok(Login1SessionProxy::builder(system)
        .path(path)?
        .build()
        .await?)
}

#[cfg(test)]
mod tests;
