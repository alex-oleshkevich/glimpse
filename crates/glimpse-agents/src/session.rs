use std::collections::HashMap;
use std::time::Duration;

use gettextrs::gettext;
use glimpse_dbus::DEADLINE;
use glimpse_dbus::freedesktop_notifications::FreedesktopNotificationsProxy;
use glimpse_dbus::login1::{Login1InhibitorEntry, Login1ManagerProxy};
use glimpse_utils::clean;
use relm4::Sender;
use tokio::sync::oneshot;
use zbus::fdo;
use zbus::zvariant::Value;

use crate::ask::Input;
use crate::dialog::{Icon, View};

const EXECUTION_DEADLINE: Duration = Duration::from_secs(120);
const INHIBITORS_MAX: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Reboot,
    PowerOff,
}

impl Action {
    fn parse(action: &str) -> fdo::Result<Self> {
        match action {
            "reboot" => Ok(Self::Reboot),
            "power-off" => Ok(Self::PowerOff),
            _ => Err(fdo::Error::InvalidArgs(
                "expected reboot or power-off".into(),
            )),
        }
    }

    async fn capability(self, manager: &Login1ManagerProxy<'_>) -> Result<(), String> {
        let capability = match self {
            Self::Reboot => manager.can_reboot().await,
            Self::PowerOff => manager.can_power_off().await,
        }
        .map_err(|error| error.to_string())?;
        match capability.as_str() {
            "yes" | "challenge" => Ok(()),
            _ => Err(gettext(
                "The requested power action is unavailable or inhibited",
            )),
        }
    }

    fn view(self, details: Vec<(String, String)>) -> View {
        let (icon, title, confirm) = match self {
            Self::Reboot => (Icon::Reboot, gettext("Restart?"), gettext("Restart")),
            Self::PowerOff => (Icon::PowerOff, gettext("Power off?"), gettext("Power off")),
        };
        View {
            icon,
            title,
            body: gettext("Unsaved work in open apps may be lost"),
            accounts: Vec::new(),
            username: None,
            secret: None,
            autoconnect: None,
            code: None,
            details,
            note: None,
            busy: false,
            confirm: Some(confirm),
            cancel: gettext("Cancel"),
        }
    }
}

pub struct Request {
    pub(crate) action: Action,
    pub(crate) reply: oneshot::Sender<fdo::Result<()>>,
}

pub(crate) struct Pending {
    pub id: u64,
    pub action: Action,
    pub view: Option<View>,
}

#[derive(Default)]
pub(crate) struct State {
    pub locked: Option<bool>,
    pub pending: Option<Pending>,
    pub executing: bool,
    next: u64,
}

impl State {
    pub fn admit(&mut self, request: Request, auth: bool) -> Option<(u64, Action)> {
        let error = if auth || self.pending.is_some() || self.executing {
            Some(fdo::Error::LimitsExceeded(
                "another prompt or power action is active".into(),
            ))
        } else {
            match self.locked {
                Some(false) => None,
                Some(true) => Some(fdo::Error::AccessDenied("the session is locked".into())),
                None => Some(fdo::Error::Failed(
                    "the session lock state is unknown".into(),
                )),
            }
        };
        if let Some(error) = error {
            let _ = request.reply.send(Err(error));
            return None;
        }
        if request.reply.send(Ok(())).is_err() {
            return None;
        }
        self.next = self.next.wrapping_add(1);
        let id = self.next;
        self.pending = Some(Pending {
            id,
            action: request.action,
            view: None,
        });
        Some((id, request.action))
    }

    pub fn take(&mut self, id: u64) -> Option<Pending> {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.id == id)
        {
            self.pending.take()
        } else {
            None
        }
    }
}

pub struct Server {
    input: Sender<Input>,
}

impl Server {
    pub fn new(input: Sender<Input>) -> Self {
        Self { input }
    }
}

#[zbus::interface(name = "me.aresa.Glimpse.Session")]
impl Server {
    async fn request_action(&self, action: &str) -> fdo::Result<()> {
        let action = Action::parse(action)?;
        let (reply, answer) = oneshot::channel();
        self.input
            .send(Input::SessionRequest(Request { action, reply }))
            .map_err(|_| fdo::Error::Failed("the dialog service is unavailable".into()))?;
        match tokio::time::timeout(DEADLINE, answer).await {
            Ok(Ok(result)) => result,
            _ => Err(fdo::Error::Failed(
                "the dialog service did not answer".into(),
            )),
        }
    }
}

fn relevant(inhibitor: &Login1InhibitorEntry) -> bool {
    inhibitor.0.split(':').any(|what| what == "shutdown")
        && matches!(inhibitor.3.as_str(), "delay" | "block" | "block-weak")
}

fn details(inhibitors: &[Login1InhibitorEntry]) -> Vec<(String, String)> {
    inhibitors
        .iter()
        .filter(|entry| relevant(entry))
        .take(INHIBITORS_MAX)
        .map(|(_, who, why, mode, _, _)| {
            let kind = match mode.as_str() {
                "delay" => gettext("Delay"),
                _ => gettext("Block"),
            };
            (
                clean(
                    &gettext("{kind}: {application}")
                        .replace("{kind}", &kind)
                        .replace("{application}", &clean(who, 64)),
                    100,
                ),
                clean(why, 240),
            )
        })
        .collect()
}

pub(crate) async fn prepare(system: &zbus::Connection, action: Action) -> Result<View, String> {
    tokio::time::timeout(DEADLINE, async {
        let manager = Login1ManagerProxy::new(system)
            .await
            .map_err(|error| error.to_string())?;
        action.capability(&manager).await?;
        let inhibitors = manager
            .list_inhibitors()
            .await
            .map_err(|error| error.to_string())?;
        Ok(action.view(details(&inhibitors)))
    })
    .await
    .map_err(|_| gettext("The power service did not answer in time"))?
}

pub(crate) async fn execute(system: &zbus::Connection, action: Action) -> Result<(), String> {
    let manager = tokio::time::timeout(DEADLINE, async {
        let manager = Login1ManagerProxy::new(system)
            .await
            .map_err(|error| error.to_string())?;
        action.capability(&manager).await?;
        let session = crate::lock::session(system)
            .await
            .map_err(|error| error.to_string())?;
        if session
            .locked_hint()
            .await
            .map_err(|error| error.to_string())?
        {
            return Err(gettext("The session is locked"));
        }
        Ok(manager)
    })
    .await
    .map_err(|_| gettext("The power service did not answer in time"))??;
    tokio::time::timeout(EXECUTION_DEADLINE, async {
        match action {
            Action::Reboot => manager.reboot(true).await,
            Action::PowerOff => manager.power_off(true).await,
        }
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| gettext("The power service did not answer in time"))?
}

pub(crate) async fn notify(session: &zbus::Connection, reason: &str) {
    let summary = gettext("The power action failed");
    let body = gtk4::glib::markup_escape_text(&clean(reason, 240));
    let result = tokio::time::timeout(DEADLINE, async {
        let proxy = FreedesktopNotificationsProxy::new(session).await?;
        proxy
            .notify(
                "glimpse-agents",
                0,
                "system-shutdown-symbolic",
                &summary,
                &body,
                &[],
                HashMap::from([("urgency", Value::U8(1))]),
                -1,
            )
            .await
    })
    .await;
    if !matches!(result, Ok(Ok(_))) {
        tracing::warn!(reason = %body, "cannot report power action failure");
    }
}

#[cfg(test)]
pub(crate) mod tests;
