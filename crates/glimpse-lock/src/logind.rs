use std::time::Duration;

use anyhow::{Context as _, Result};
use futures_util::stream::BoxStream;
use futures_util::{StreamExt, TryFutureExt};
use glimpse_dbus::Delivery;
use glimpse_dbus::login1::{Login1ManagerProxy, Login1SessionProxy, session_path};
use tokio::sync::mpsc;
use zbus::zvariant::OwnedFd;

use crate::lifecycle::{Input, sleep_wait};
use crate::session::{self, Action};

const CALL_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_MIN: Duration = Duration::from_secs(1);
const RETRY_MAX: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub enum Request {
    SetLockedHint(bool),
    TakeInhibitor,
    ReleaseInhibitor,
    FetchSessionActions,
    PerformSessionAction(Action, u64),
}

#[derive(Debug, Clone, Copy)]
pub struct Ready {
    pub initial_hint: Option<bool>,
    pub sleep_wait: Duration,
    pub preparing_for_sleep: bool,
}

pub enum Event {
    Ready(Ready),
    Lifecycle(Input),
    SessionAnswers(session::Answers),
    SessionActionResult(Action, u64, session::Outcome),
    BlockInhibitedChanged,
}

pub fn spawn(
    events: impl Fn(Event) + Send + Sync + Clone + 'static,
) -> mpsc::UnboundedSender<Request> {
    let (requests, mut receiver) = mpsc::unbounded_channel();
    relm4::spawn(async move {
        let mut delay = RETRY_MIN;
        let mut initial = true;
        loop {
            match connect(initial).await {
                Ok((logind, ready)) => {
                    delay = RETRY_MIN;
                    events(Event::Ready(ready));
                    initial = false;
                    match logind.follow(&mut receiver, events.clone()).await {
                        Ended::Stop => return,
                        Ended::OwnerChanged => {}
                        Ended::Lost => {
                            tokio::time::sleep(delay).await;
                            delay = (delay * 2).min(RETRY_MAX);
                        }
                    }
                }
                Err(error) => {
                    tracing::error!(
                        retry = ?delay,
                        "logind is unreachable, so nothing can request a lock: {error:#}"
                    );
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(RETRY_MAX);
                }
            }
        }
    });
    requests
}

enum Ended {
    Stop,
    OwnerChanged,
    Lost,
}

struct Logind {
    manager: Login1ManagerProxy<'static>,
    session: Login1SessionProxy<'static>,
    locks: BoxStream<'static, ()>,
    unlocks: BoxStream<'static, ()>,
    sleeps: BoxStream<'static, Delivery<bool>>,
    owners: BoxStream<'static, ()>,
}

async fn connect(initial: bool) -> Result<(Logind, Ready)> {
    let logind = tokio::time::timeout(CALL_TIMEOUT, async {
        let bus = zbus::Connection::system()
            .await
            .context("connect to the system bus")?;
        let dbus = zbus::fdo::DBusProxy::new(&bus).await?;
        let owners = glimpse_dbus::wake(
            dbus.receive_name_owner_changed_with_args(&[(0, "org.freedesktop.login1")])
                .await?
                .filter_map(|signal| async move {
                    signal.args().ok()?.old_owner().is_some().then_some(())
                }),
        );
        let name = zbus::names::WellKnownName::try_from("org.freedesktop.login1")?;
        let owner = match dbus.get_name_owner(name.clone().into()).await {
            Ok(owner) => owner,
            Err(zbus::fdo::Error::NameHasNoOwner(_)) => {
                dbus.start_service_by_name(name.clone(), 0).await?;
                dbus.get_name_owner(name.into()).await?
            }
            Err(error) => return Err(error.into()),
        };
        let manager = Login1ManagerProxy::builder(&bus)
            .destination(owner.clone())?
            .build()
            .await?;
        let path = session_path(&bus).await.map_err(anyhow::Error::msg)?;
        tracing::info!(session = %path, "following the login session");
        let session = Login1SessionProxy::builder(&bus)
            .destination(owner)?
            .path(path)?
            .build()
            .await?;
        let (locks, unlocks, sleeps) =
            tokio::try_join!(
                session.receive_lock().map_ok(glimpse_dbus::wake),
                session.receive_unlock().map_ok(glimpse_dbus::wake),
                manager.receive_prepare_for_sleep().map_ok(|stream| {
                    glimpse_dbus::events(stream.filter_map(|signal| async move {
                        signal.args().ok().map(|args| args.start)
                    }))
                }),
            )
            .context("subscribe to logind signals")?;
        Ok::<_, anyhow::Error>(Logind {
            manager,
            session,
            locks,
            unlocks,
            sleeps,
            owners,
        })
    })
    .await
    .context("logind did not answer in time")??;
    let mandatory = async {
        let initial_hint = if initial {
            Some(
                logind
                    .session
                    .locked_hint()
                    .await
                    .context("read LockedHint")?,
            )
        } else {
            None
        };
        let preparing = preparing_for_sleep(&logind.manager)
            .await
            .context("read PreparingForSleep")?;
        Ok::<_, anyhow::Error>((initial_hint, preparing))
    };
    let (mandatory, delay) = tokio::join!(
        tokio::time::timeout(CALL_TIMEOUT, mandatory),
        bounded(
            "InhibitDelayMaxUSec",
            logind
                .manager
                .inner()
                .get_property::<u64>("InhibitDelayMaxUSec"),
        ),
    );
    let (initial_hint, preparing_for_sleep) =
        mandatory.context("mandatory logind state did not answer in time")??;
    let delay = delay.and_then(|reply| {
        reply
            .inspect_err(|error| tracing::warn!(%error, "cannot read InhibitDelayMaxUSec"))
            .ok()
    });
    Ok((
        logind,
        Ready {
            initial_hint,
            sleep_wait: sleep_wait(delay),
            preparing_for_sleep,
        },
    ))
}

impl Logind {
    async fn follow(
        self,
        requests: &mut mpsc::UnboundedReceiver<Request>,
        events: impl Fn(Event) + Clone + Send + Sync + 'static,
    ) -> Ended {
        let Logind {
            manager,
            session,
            mut locks,
            mut unlocks,
            mut sleeps,
            mut owners,
        } = self;
        let mut blocks = manager.receive_block_inhibited_changed().await;
        let mut worker = std::pin::pin!(serve(manager.clone(), session, requests, events.clone()));
        let mut resync = std::pin::pin!(None);
        loop {
            tokio::select! {
                () = &mut worker => return Ended::Stop,
                signal = locks.next() => match signal {
                    Some(()) => events(Event::Lifecycle(Input::LockRequested)),
                    None => {
                        events(Event::Lifecycle(Input::BackendLost));
                        ended("Session.Lock");
                        return Ended::Lost;
                    }
                },
                signal = unlocks.next() => match signal {
                    Some(()) => events(Event::Lifecycle(Input::UnlockRequested)),
                    None => {
                        events(Event::Lifecycle(Input::BackendLost));
                        ended("Session.Unlock");
                        return Ended::Lost;
                    }
                },
                signal = sleeps.next() => match signal {
                    Some(Delivery::Item(start)) => {
                        if resync.as_ref().get_ref().is_some() {
                            resync.set(Some(resync_preparing(manager.clone())));
                        }
                        events(Event::Lifecycle(Input::PrepareForSleep(start)));
                    }
                    Some(Delivery::Missed) => {
                        events(Event::Lifecycle(Input::BackendLost));
                        resync.set(Some(resync_preparing(manager.clone())));
                    }
                    None => {
                        events(Event::Lifecycle(Input::BackendLost));
                        ended("PrepareForSleep");
                        return Ended::Lost;
                    }
                },
                preparing = async {
                    match resync.as_mut().as_pin_mut() {
                        Some(resync) => resync.await,
                        None => std::future::pending().await,
                    }
                }, if resync.as_ref().get_ref().is_some() => {
                    resync.set(None);
                    events(Event::Lifecycle(Input::BackendReady {
                        initial_hint: None,
                        preparing_for_sleep: preparing,
                    }));
                },
                change = blocks.next() => match change {
                    Some(_) => events(Event::BlockInhibitedChanged),
                    None => {
                        events(Event::Lifecycle(Input::BackendLost));
                        ended("BlockInhibited");
                        return Ended::Lost;
                    }
                },
                owner = owners.next() => {
                    events(Event::Lifecycle(Input::BackendLost));
                    if owner.is_some() {
                        tracing::warn!("the logind owner changed; reconnecting");
                        return Ended::OwnerChanged;
                    }
                    ended("NameOwnerChanged");
                    return Ended::Lost;
                },
            }
        }
    }
}

async fn preparing_for_sleep(manager: &Login1ManagerProxy<'_>) -> zbus::Result<bool> {
    let proxy = manager.inner();
    let reply = proxy
        .connection()
        .call_method(
            Some(proxy.destination().as_str()),
            proxy.path().as_str(),
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(proxy.interface().as_str(), "PreparingForSleep"),
        )
        .await?;
    let value: zbus::zvariant::OwnedValue = reply.body().deserialize()?;
    Ok(bool::try_from(value)?)
}

async fn resync_preparing(manager: Login1ManagerProxy<'static>) -> bool {
    let mut delay = RETRY_MIN;
    loop {
        match bounded("PreparingForSleep", preparing_for_sleep(&manager)).await {
            Some(Ok(preparing)) => return preparing,
            Some(Err(error)) => tracing::warn!(%error, "cannot read PreparingForSleep"),
            None => {}
        }
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(RETRY_MAX);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Call {
    SetLockedHint(bool),
    Inhibit,
    FetchSessionActions,
    PerformSessionAction(Action, u64),
}

fn step<F>(inhibitor: &mut Option<F>, request: Request) -> Option<Call> {
    match request {
        Request::SetLockedHint(locked) => Some(Call::SetLockedHint(locked)),
        Request::TakeInhibitor if inhibitor.is_some() => None,
        Request::TakeInhibitor => Some(Call::Inhibit),
        Request::ReleaseInhibitor => {
            if inhibitor.take().is_some() {
                tracing::info!("released the sleep inhibitor");
            }
            None
        }
        Request::FetchSessionActions => Some(Call::FetchSessionActions),
        Request::PerformSessionAction(action, id) => Some(Call::PerformSessionAction(action, id)),
    }
}

async fn serve(
    manager: Login1ManagerProxy<'static>,
    session: Login1SessionProxy<'static>,
    requests: &mut mpsc::UnboundedReceiver<Request>,
    events: impl Fn(Event) + Clone + Send + Sync + 'static,
) {
    let mut inhibitor: Option<OwnedFd> = None;
    while let Some(request) = requests.recv().await {
        match step(&mut inhibitor, request) {
            None => {}
            Some(Call::SetLockedHint(locked)) => {
                match bounded("SetLockedHint", session.set_locked_hint(locked)).await {
                    Some(Ok(())) => tracing::info!(locked, "SetLockedHint"),
                    Some(Err(error)) => tracing::error!(locked, %error, "SetLockedHint failed"),
                    None => {}
                }
            }
            Some(Call::Inhibit) => {
                let reply = manager.inhibit(
                    "sleep",
                    "glimpse-lock",
                    "Lock the screen before sleep",
                    "delay",
                );
                match bounded("Inhibit", reply).await {
                    Some(Ok(fd)) => {
                        tracing::info!("holding a sleep delay inhibitor");
                        inhibitor = Some(fd);
                    }
                    Some(Err(error)) => tracing::warn!(
                        %error,
                        "cannot take a sleep inhibitor; retrying at the next resume or unlock"
                    ),
                    None => {}
                }
            }
            Some(Call::FetchSessionActions) => {
                let manager = manager.clone();
                let events = events.clone();
                relm4::spawn(async move {
                    if let Some(answers) = fetch_session_answers(&manager).await {
                        events(Event::SessionAnswers(answers));
                    }
                });
            }
            Some(Call::PerformSessionAction(action, id)) => {
                let manager = manager.clone();
                let events = events.clone();
                relm4::spawn(async move {
                    let outcome = perform_session_action(&manager, action).await;
                    events(Event::SessionActionResult(action, id, outcome));
                });
            }
        }
    }
}

async fn bounded<T>(method: &str, reply: impl Future<Output = T>) -> Option<T> {
    let reply = tokio::time::timeout(CALL_TIMEOUT, reply).await.ok();
    if reply.is_none() {
        tracing::warn!(method, timeout = ?CALL_TIMEOUT, "logind did not answer in time");
    }
    reply
}

fn ended(signal: &str) {
    tracing::error!(
        signal,
        "the logind signal stream ended; the system bus is gone"
    );
}

async fn fetch_session_answers(manager: &Login1ManagerProxy<'static>) -> Option<session::Answers> {
    let reply = bounded("SessionActions", async {
        tokio::try_join!(
            manager.can_suspend(),
            manager.can_reboot(),
            manager.can_power_off(),
            manager.list_inhibitors(),
        )
    })
    .await?;
    match reply {
        Ok((suspend, reboot, power_off, inhibitors)) => Some(session::Answers {
            suspend,
            reboot,
            power_off,
            inhibitors,
        }),
        Err(error) => {
            tracing::warn!(%error, "cannot read session action state from logind");
            None
        }
    }
}

async fn perform_session_action(
    manager: &Login1ManagerProxy<'static>,
    action: Action,
) -> session::Outcome {
    let result = match action {
        Action::Suspend => bounded("Suspend", manager.suspend(false)).await,
        Action::Reboot => bounded("Reboot", manager.reboot(false)).await,
        Action::PowerOff => bounded("PowerOff", manager.power_off(false)).await,
    };
    match result {
        Some(Ok(())) => session::Outcome::Succeeded,
        Some(Err(error)) => {
            tracing::warn!(?action, %error, "session action failed");
            session::Outcome::Failed
        }
        None => {
            tracing::warn!(
                ?action,
                "logind did not answer in time; the action may still happen"
            );
            session::Outcome::TimedOut
        }
    }
}

#[cfg(test)]
mod tests;
