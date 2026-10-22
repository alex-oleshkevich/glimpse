use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use gettextrs::gettext;
use relm4::Sender;
use tokio::sync::oneshot;
use zbus::zvariant::{OwnedValue, Value};

use glimpse_dbus::DEADLINE;
use glimpse_dbus::accounts::{AccountsProxy, AccountsUserProxy};
use glimpse_dbus::freedesktop_notifications::FreedesktopNotificationsProxy;
use glimpse_dbus::polkit::AuthorityProxy;

use crate::ask::{Ask, Input, Prompt, Reply, Source};
use crate::polkit::{Outcome, request, resolve_identities, unix_users};

pub const PATH: &str = "/org/freedesktop/PolicyKit1/AuthenticationAgent";

const AGENTS_NAME: &str = "me.aresa.Glimpse.Agents";
const ALREADY_EXISTS: &str = "org.freedesktop.PolicyKit1.Error.Failed";

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.PolicyKit1.Error")]
enum AgentError {
    Cancelled(String),
}

pub struct Agent {
    system: zbus::Connection,
    input: Sender<Input>,
    cancelled: Arc<Mutex<HashSet<String>>>,
}

impl Agent {
    pub(crate) fn new(
        system: zbus::Connection,
        input: Sender<Input>,
        cancelled: Arc<Mutex<HashSet<String>>>,
    ) -> Self {
        Self {
            system,
            input,
            cancelled,
        }
    }

    fn cancelled(&self) -> std::sync::MutexGuard<'_, HashSet<String>> {
        self.cancelled
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn take_cancelled(&self, cookie: &str) -> bool {
        self.cancelled().remove(cookie)
    }

    async fn begin(&self, cookie: &str, ask: Ask) -> Result<(), AgentError> {
        if self.take_cancelled(cookie) {
            return Err(cancelled());
        }
        if self.input.send(Input::Ask(ask)).is_err() {
            return Err(cancelled());
        }
        Ok(())
    }
}

#[zbus::interface(name = "org.freedesktop.PolicyKit1.AuthenticationAgent")]
impl Agent {
    #[allow(clippy::too_many_arguments)]
    async fn begin_authentication(
        &self,
        action_id: String,
        message: String,
        _icon_name: String,
        details: HashMap<String, String>,
        cookie: String,
        identities: Vec<(String, HashMap<String, OwnedValue>)>,
    ) -> Result<(), AgentError> {
        let identities = admin_identities(&self.system, &identities).await;
        let (reply, answer) = oneshot::channel();
        let ask = Ask {
            source: Source::Polkit,
            key: cookie.clone(),
            prompt: Prompt::Polkit(
                request(&action_id, &message, details, cookie.clone(), identities).await,
            ),
            reply: Reply::Polkit(reply),
        };
        self.begin(&cookie, ask).await?;
        if self.cancelled().contains(&cookie) {
            let _ = self.input.send(Input::Withdraw {
                source: Source::Polkit,
                key: Some(cookie.clone()),
            });
        }
        let outcome = answer.await;
        self.take_cancelled(&cookie);
        match outcome {
            Ok(Outcome::Authorized) => Ok(()),
            Ok(Outcome::Cancelled) | Err(_) => Err(cancelled()),
        }
    }

    async fn cancel_authentication(&self, cookie: String) -> Result<(), AgentError> {
        self.cancelled().insert(cookie.clone());
        let _ = self.input.send(Input::Withdraw {
            source: Source::Polkit,
            key: Some(cookie),
        });
        Ok(())
    }
}

struct Cancelling;

#[zbus::interface(name = "org.freedesktop.PolicyKit1.AuthenticationAgent")]
impl Cancelling {
    #[allow(clippy::too_many_arguments)]
    async fn begin_authentication(
        &self,
        _action_id: String,
        _message: String,
        _icon_name: String,
        _details: HashMap<String, String>,
        _cookie: String,
        _identities: Vec<(String, HashMap<String, OwnedValue>)>,
    ) -> Result<(), AgentError> {
        Err(cancelled())
    }

    async fn cancel_authentication(&self, _cookie: String) -> Result<(), AgentError> {
        Ok(())
    }
}

fn cancelled() -> AgentError {
    AgentError::Cancelled("cancelled".to_owned())
}

pub(crate) fn duplicate(error: &zbus::Error) -> bool {
    match error {
        zbus::Error::MethodError(name, detail, _) => {
            name.as_str() == ALREADY_EXISTS
                && detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("already exists"))
        }
        _ => false,
    }
}

async fn notify_another_agent(session: &zbus::Connection) -> Result<(), String> {
    let proxy = FreedesktopNotificationsProxy::new(session)
        .await
        .map_err(|error| error.to_string())?;
    let summary =
        gettext("Another authentication agent is running; glimpse can't ask for admin passwords");
    let hints = HashMap::from([("urgency", Value::U8(1))]);
    let notify = proxy.notify(
        "glimpse-agents",
        0,
        "dialog-password-symbolic",
        &summary,
        "",
        &[],
        hints,
        -1,
    );
    match tokio::time::timeout(DEADLINE, notify).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(_) => Err("the notification daemon did not answer in time".to_owned()),
    }
}

fn locale() -> String {
    match std::env::var("LANG") {
        Ok(lang) if !lang.is_empty() => lang,
        _ => "C".to_owned(),
    }
}

fn subject<'a>(session_id: &'a str) -> (&'static str, HashMap<&'static str, Value<'a>>) {
    let mut details = HashMap::new();
    details.insert("session-id", Value::from(session_id));
    ("unix-session", details)
}

pub(crate) async fn register_with(system: &zbus::Connection, session_id: &str) -> zbus::Result<()> {
    AuthorityProxy::new(system)
        .await?
        .register_authentication_agent(subject(session_id), &locale(), PATH)
        .await
}

pub(crate) async fn unregister_with(
    system: &zbus::Connection,
    session_id: &str,
) -> zbus::Result<()> {
    AuthorityProxy::new(system)
        .await?
        .unregister_authentication_agent(subject(session_id), PATH)
        .await
}

pub async fn serve(
    system: zbus::Connection,
    input: Sender<Input>,
    cancelled: Arc<Mutex<HashSet<String>>>,
) {
    let session = match tokio::time::timeout(DEADLINE, zbus::Connection::session()).await {
        Ok(Ok(session)) => Some(session),
        Ok(Err(error)) => {
            tracing::warn!(%error, "no session bus for the polkit notice");
            None
        }
        Err(_) => {
            tracing::warn!("the session bus did not answer in time");
            None
        }
    };
    let session_id = match session_id_of(&system).await {
        Ok(id) => id,
        Err(error) => {
            tracing::warn!(%error, "no logind session; the polkit agent is not registered");
            return;
        }
    };
    serve_with(system, session, input, &session_id, cancelled).await;
}

pub(crate) async fn serve_with(
    system: zbus::Connection,
    session: Option<zbus::Connection>,
    input: Sender<Input>,
    session_id: &str,
    cancelled: Arc<Mutex<HashSet<String>>>,
) {
    if let Err(error) = system
        .object_server()
        .at(PATH, Agent::new(system.clone(), input, cancelled))
        .await
    {
        tracing::warn!(%error, "could not export the polkit agent");
        return;
    }
    match register_with(&system, session_id).await {
        Ok(()) => tracing::info!("polkit authentication agent registered"),
        Err(error) if duplicate(&error) => {
            tracing::warn!("another authentication agent is already registered");
            if let Some(session) = session.as_ref()
                && let Err(error) = notify_another_agent(session).await
            {
                tracing::warn!(%error, "could not post the polkit notice");
            }
        }
        Err(error) => tracing::warn!(%error, "could not register the polkit agent"),
    }
}

pub async fn registration_report(
    system: &zbus::Connection,
    session: &zbus::Connection,
) -> Result<String, String> {
    match tokio::time::timeout(DEADLINE, name_owned(session)).await {
        Ok(Ok(true)) => return Ok("running".to_owned()),
        Ok(Ok(false)) => {}
        Ok(Err(error)) => return Err(format!("failed: {error}")),
        Err(_) => return Err("failed: the session bus did not answer in time".to_owned()),
    }
    let id = session_id_of(system)
        .await
        .map_err(|error| format!("failed: {error}"))?;
    probe(system, &id).await
}

pub(crate) async fn probe(system: &zbus::Connection, session_id: &str) -> Result<String, String> {
    let exported = system
        .object_server()
        .at(PATH, Cancelling)
        .await
        .map_err(|error| format!("failed: {error}"))?;
    let outcome = match register_with(system, session_id).await {
        Ok(()) => unregister_with(system, session_id)
            .await
            .map(|()| "ok".to_owned())
            .map_err(|error| format!("failed: {error}")),
        Err(error) if duplicate(&error) => Err("another agent".to_owned()),
        Err(error) => Err(format!("failed: {error}")),
    };
    if exported {
        let _ = system.object_server().remove::<Cancelling, _>(PATH).await;
    }
    outcome
}

async fn name_owned(session: &zbus::Connection) -> zbus::Result<bool> {
    let name = zbus::names::BusName::try_from(AGENTS_NAME)
        .map_err(|error| zbus::Error::Failure(format!("invalid agents bus name: {error}")))?;
    Ok(zbus::fdo::DBusProxy::new(session)
        .await?
        .name_has_owner(name)
        .await?)
}

async fn session_id_of(system: &zbus::Connection) -> Result<String, String> {
    let session = match tokio::time::timeout(DEADLINE, crate::lock::session(system)).await {
        Ok(result) => result.map_err(|error| format!("{error:#}"))?,
        Err(_) => return Err("logind session timed out".to_owned()),
    };
    match tokio::time::timeout(DEADLINE, session.id()).await {
        Ok(Ok(id)) => Ok(id),
        Ok(Err(error)) => Err(error.to_string()),
        Err(_) => Err("logind session id timed out".to_owned()),
    }
}

async fn admin_identities(
    system: &zbus::Connection,
    identities: &[(String, HashMap<String, OwnedValue>)],
) -> Vec<crate::polkit::Identity> {
    let uids = unix_users(identities);
    let passwd = tokio::fs::read_to_string("/etc/passwd")
        .await
        .unwrap_or_default();
    let names = real_names(system, &uids).await;
    resolve_identities(&uids, &passwd, &names, glimpse_dbus::login1::current_uid())
}

async fn real_names(system: &zbus::Connection, uids: &[u32]) -> HashMap<u32, String> {
    let Ok(Ok(accounts)) = tokio::time::timeout(DEADLINE, AccountsProxy::new(system)).await else {
        return HashMap::new();
    };
    let mut names = HashMap::new();
    for uid in uids {
        let Ok(Ok(path)) =
            tokio::time::timeout(DEADLINE, accounts.find_user_by_id(i64::from(*uid))).await
        else {
            continue;
        };
        let Ok(user) = async { AccountsUserProxy::builder(system).path(path)?.build().await }.await
        else {
            continue;
        };
        let Ok(Ok(name)) = tokio::time::timeout(DEADLINE, user.real_name()).await else {
            continue;
        };
        names.insert(*uid, name);
    }
    names
}
