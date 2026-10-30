use std::collections::{HashMap, HashSet};
use std::pin::pin;
use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use gettextrs::gettext;
use glimpse_compositors::client::Compositor1Proxy;
use glimpse_utils::clean;
use relm4::Sender;
use tokio::io::unix::AsyncFd;
use tokio::sync::{Notify, oneshot};
use zbus::zvariant::{OwnedValue, Value};

use glimpse_dbus::accounts::{AccountsProxy, AccountsUserProxy};
use glimpse_dbus::freedesktop_notifications::FreedesktopNotificationsProxy;
use glimpse_dbus::polkit::AuthorityProxy;
use glimpse_dbus::{DEADLINE, Delivery};

use crate::ask::{Ask, Input, Prompt, Reply, Source};
use crate::notifications::{self, Signal};
use crate::polkit::requester::{self, Background};
use crate::polkit::{Outcome, request, resolve_identities, unix_users};

pub const PATH: &str = "/org/freedesktop/PolicyKit1/AuthenticationAgent";

const AUTHENTICATE: &str = "authenticate";
const FOCUS: &str = "focus";
const AGENTS_NAME: &str = "me.aresa.Glimpse.Agents";
const ALREADY_EXISTS: &str = "org.freedesktop.PolicyKit1.Error.Failed";

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.PolicyKit1.Error")]
enum AgentError {
    Cancelled(String),
}

pub struct Agent {
    system: zbus::Connection,
    session: Option<zbus::Connection>,
    input: Sender<Input>,
    cancelled: Arc<Mutex<HashSet<String>>>,
    cancel: Notify,
}

impl Agent {
    pub(crate) fn new(
        system: zbus::Connection,
        session: Option<zbus::Connection>,
        input: Sender<Input>,
        cancelled: Arc<Mutex<HashSet<String>>>,
    ) -> Self {
        Self {
            system,
            input,
            cancelled,
            session,
            cancel: Notify::new(),
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

    async fn defer(
        &self,
        session: &zbus::Connection,
        cookie: &str,
        body: &str,
        watched: Option<u32>,
        background: Background,
    ) -> bool {
        let mut gone = Box::pin(exited(watched));
        if futures_util::FutureExt::now_or_never(&mut gone).is_some() {
            return false;
        }
        let Ok(proxy) = FreedesktopNotificationsProxy::new(session).await else {
            return true;
        };
        let capabilities = tokio::time::timeout(DEADLINE, proxy.get_capabilities()).await;
        let Ok(Ok(capabilities)) = capabilities else {
            return true;
        };
        if !capabilities.iter().any(|name| name == "actions") {
            return true;
        }
        let Ok(mut signals) = notifications::signals(session).await else {
            return true;
        };
        let Background { window, app } = background;
        let authenticate = gettext("Authenticate");
        let show = window.map(|_| match &app {
            Some(app) => gettext("Show {app}").replace("{app}", &app.name),
            None => gettext("Show Window"),
        });
        let mut actions = vec![
            "default",
            authenticate.as_str(),
            AUTHENTICATE,
            authenticate.as_str(),
        ];
        if let Some(show) = &show {
            actions.extend([FOCUS, show.as_str()]);
        }
        let mut hints = HashMap::from([
            ("urgency", Value::U8(1)),
            ("resident", Value::Bool(true)),
            ("desktop-entry", Value::from(AGENTS_NAME)),
        ]);
        if let Some(image) = app.as_ref().and_then(|app| app.image.as_deref()) {
            hints.insert("image-path", Value::from(image));
        }
        let summary = gettext("Authentication required");
        let header = gettext("Authentication");
        let text = gtk4::glib::markup_escape_text(&clean(body, 240));
        let sent = tokio::time::timeout(
            DEADLINE,
            proxy.notify(
                &header,
                0,
                "dialog-password-symbolic",
                &summary,
                &text,
                &actions,
                hints,
                0,
            ),
        )
        .await;
        let Ok(Ok(id)) = sent else {
            return true;
        };
        let show = loop {
            let mut cancel = pin!(self.cancel.notified());
            cancel.as_mut().enable();
            if self.cancelled().contains(cookie) {
                break false;
            }
            tokio::select! {
                delivery = signals.next() => match delivery {
                    Some(Delivery::Item(Signal::Action(got, key))) if got == id && key == FOCUS => {
                        if let Some(window) = window {
                            focus(session, window).await;
                        }
                    }
                    Some(Delivery::Item(Signal::Action(got, _))) if got == id => break true,
                    Some(Delivery::Item(Signal::Closed(got, 2))) if got == id => return false,
                    Some(Delivery::Item(Signal::Closed(got, _))) if got == id => return true,
                    Some(Delivery::Item(Signal::OwnerChanged)) => break true,
                    Some(Delivery::Item(_)) => {}
                    Some(Delivery::Missed) | None => break true,
                },
                () = &mut cancel => {}
                () = &mut gone => break false,
            }
        };
        let _ = tokio::time::timeout(DEADLINE, proxy.close_notification(id)).await;
        show
    }
}

async fn focus(session: &zbus::Connection, window: u64) {
    let focused = tokio::time::timeout(DEADLINE, async {
        Compositor1Proxy::new(session)
            .await?
            .focus_window(window)
            .await
    })
    .await;
    if !matches!(focused, Ok(Ok(()))) {
        tracing::debug!(window, "could not focus the requesting window");
    }
}

async fn exited(pid: Option<u32>) {
    let Some(pid) = pid
        .and_then(|pid| i32::try_from(pid).ok())
        .and_then(rustix::process::Pid::from_raw)
    else {
        return std::future::pending().await;
    };
    match rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::empty()) {
        Ok(fd) => match AsyncFd::new(fd) {
            Ok(fd) => {
                let _ = fd.readable().await;
            }
            Err(_) => std::future::pending().await,
        },
        Err(rustix::io::Errno::SRCH) => {}
        Err(_) => std::future::pending().await,
    }
}

fn pid_detail(details: &HashMap<String, String>, key: &str) -> Option<u32> {
    details.get(key)?.parse::<u32>().ok().filter(|pid| *pid > 1)
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
        let subject = pid_detail(&details, "polkit.subject-pid");
        let caller = pid_detail(&details, "polkit.caller-pid");
        let identities = admin_identities(&self.system, &identities).await;
        let (reply, answer) = oneshot::channel();
        let request = request(&action_id, &message, details, cookie.clone(), identities).await;
        let body = request.message.clone();
        let ask = Ask {
            source: Source::Polkit,
            key: cookie.clone(),
            prompt: Prompt::Polkit(request),
            reply: Reply::Polkit(reply),
        };
        if let Some(session) = self.session.as_ref()
            && let Some(background) = requester::background(session, subject.or(caller)).await
            && !self
                .defer(session, &cookie, &body, caller.or(subject), background)
                .await
        {
            self.take_cancelled(&cookie);
            return Err(cancelled());
        }
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
        self.cancel.notify_waiters();
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
        .at(
            PATH,
            Agent::new(system.clone(), session.clone(), input, cancelled),
        )
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
