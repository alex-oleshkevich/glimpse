use std::time::Duration;

use anyhow::{Context as _, Result};
use futures_util::StreamExt;
use glimpse_dbus::login1::{Login1ManagerProxy, Login1SessionProxy, session_path};
use tokio::sync::mpsc;
use zbus::zvariant::OwnedFd;

use crate::lifecycle::{Input, sleep_wait};
use crate::session::{self, Action};

const CALL_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub enum Request {
    SetLockedHint(bool),
    TakeInhibitor,
    ReleaseInhibitor,
    FetchSessionActions,
    PerformSessionAction(Action, u64),
}

pub enum Event {
    Start {
        locked_hint: bool,
        sleep_wait: Duration,
    },
    Lifecycle(Input),
    SessionAnswers(session::Answers),
    SessionActionResult(Action, u64, session::Outcome),
    BlockInhibitedChanged,
}

pub fn spawn(
    events: impl Fn(Event) + Send + Sync + Clone + 'static,
) -> mpsc::UnboundedSender<Request> {
    let (requests, receiver) = mpsc::unbounded_channel();
    relm4::spawn(async move {
        match connect().await {
            Ok(logind) => logind.follow(receiver, events).await,
            Err(error) => {
                tracing::error!("logind is unreachable, so nothing can request a lock: {error:#}");
                events(Event::Start {
                    locked_hint: false,
                    sleep_wait: sleep_wait(None),
                });
            }
        }
    });
    requests
}

struct Logind {
    manager: Login1ManagerProxy<'static>,
    session: Login1SessionProxy<'static>,
}

async fn connect() -> Result<Logind> {
    tokio::time::timeout(CALL_TIMEOUT, async {
        let bus = zbus::Connection::system()
            .await
            .context("connect to the system bus")?;
        let manager = Login1ManagerProxy::new(&bus).await?;
        let path = session_path(&bus).await.map_err(anyhow::Error::msg)?;
        tracing::info!(session = %path, "following the login session");
        let session = Login1SessionProxy::builder(&bus)
            .path(path)?
            .build()
            .await?;
        Ok(Logind { manager, session })
    })
    .await
    .context("logind did not answer in time")?
}

impl Logind {
    async fn follow(
        self,
        requests: mpsc::UnboundedReceiver<Request>,
        events: impl Fn(Event) + Clone + Send + Sync + 'static,
    ) {
        let streams = tokio::time::timeout(CALL_TIMEOUT, async {
            tokio::try_join!(
                self.session.receive_lock(),
                self.session.receive_unlock(),
                self.manager.receive_prepare_for_sleep(),
            )
        })
        .await
        .context("logind did not answer in time")
        .and_then(|streams| streams.map_err(anyhow::Error::from));
        let (mut locks, mut unlocks, mut sleeps) = match streams {
            Ok(streams) => streams,
            Err(error) => {
                tracing::error!("cannot follow logind signals: {error:#}");
                events(Event::Start {
                    locked_hint: false,
                    sleep_wait: sleep_wait(None),
                });
                return;
            }
        };
        let (locked_hint, delay) = tokio::join!(
            bounded("LockedHint", self.session.locked_hint()),
            bounded(
                "InhibitDelayMaxUSec",
                self.manager
                    .inner()
                    .get_property::<u64>("InhibitDelayMaxUSec"),
            ),
        );
        let locked_hint = match locked_hint {
            Some(Ok(hint)) => hint,
            Some(Err(error)) => {
                tracing::warn!(%error, "cannot read LockedHint");
                false
            }
            None => false,
        };
        let delay = delay.and_then(|reply| {
            reply
                .inspect_err(|error| tracing::warn!(%error, "cannot read InhibitDelayMaxUSec"))
                .ok()
        });
        events(Event::Start {
            locked_hint,
            sleep_wait: sleep_wait(delay),
        });

        let mut blocks = self.manager.receive_block_inhibited_changed().await;

        let mut worker = relm4::spawn(serve(self.manager, self.session, requests, events.clone()));

        let (mut locking, mut unlocking, mut sleeping, mut serving, mut watching_blocks) =
            (true, true, true, true, true);
        loop {
            tokio::select! {
                outcome = &mut worker, if serving => match outcome {
                    Ok(()) => return,
                    Err(error) => {
                        tracing::error!(%error, "the logind worker died; LockedHint and the sleep inhibitor are no longer followed");
                        serving = false;
                    }
                },
                signal = locks.next(), if locking => match signal {
                    Some(_) => events(Event::Lifecycle(Input::LockRequested)),
                    None => locking = ended("Session.Lock"),
                },
                signal = unlocks.next(), if unlocking => match signal {
                    Some(_) => events(Event::Lifecycle(Input::UnlockRequested)),
                    None => unlocking = ended("Session.Unlock"),
                },
                signal = sleeps.next(), if sleeping => match signal.map(|signal| signal.args().map(|args| args.start)) {
                    Some(Ok(start)) => events(Event::Lifecycle(Input::PrepareForSleep(start))),
                    Some(Err(error)) => tracing::warn!(%error, "undecodable PrepareForSleep"),
                    None => sleeping = ended("PrepareForSleep"),
                },
                change = blocks.next(), if watching_blocks => match change {
                    Some(_) => events(Event::BlockInhibitedChanged),
                    None => watching_blocks = ended("BlockInhibited"),
                },
                else => return,
            }
        }
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
    mut requests: mpsc::UnboundedReceiver<Request>,
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

fn ended(signal: &str) -> bool {
    tracing::error!(
        signal,
        "the logind signal stream ended; the system bus is gone"
    );
    false
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
mod tests {
    use super::*;

    #[test]
    fn only_a_release_drops_the_inhibitor() {
        let mut held = Some(7);
        assert_eq!(
            step(&mut held, Request::SetLockedHint(true)),
            Some(Call::SetLockedHint(true))
        );
        assert_eq!(held, Some(7));
        assert_eq!(
            step(&mut held, Request::TakeInhibitor),
            None,
            "one is held already"
        );
        assert_eq!(held, Some(7));
        assert_eq!(step(&mut held, Request::ReleaseInhibitor), None);
        assert_eq!(held, None);
        assert_eq!(step(&mut held, Request::ReleaseInhibitor), None);
        assert_eq!(step(&mut held, Request::TakeInhibitor), Some(Call::Inhibit));
    }

    #[test]
    fn session_requests_pass_straight_through() {
        let mut held: Option<i32> = None;
        assert_eq!(
            step(&mut held, Request::FetchSessionActions),
            Some(Call::FetchSessionActions)
        );
        assert_eq!(
            step(&mut held, Request::PerformSessionAction(Action::Suspend, 7)),
            Some(Call::PerformSessionAction(Action::Suspend, 7))
        );
        assert_eq!(held, None, "a session request never touches the inhibitor");
    }
}
