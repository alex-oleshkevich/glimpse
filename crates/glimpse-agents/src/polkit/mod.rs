pub mod helper;

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub uid: u32,
    pub user: String,
    pub name: String,
}

#[derive(Clone)]
pub struct Request {
    pub action_id: String,
    pub message: String,
    pub details: Vec<(String, String)>,
    pub cookie: String,
    pub identities: Vec<Identity>,
}

impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Request")
            .field("action_id", &self.action_id)
            .field("message", &self.message)
            .field("details", &self.details)
            .field("cookie", &"<redacted>")
            .field("identities", &self.identities)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Authorized,
    Cancelled,
}

pub mod agent;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gettextrs::gettext;
use glimpse_utils::clean;
use relm4::Sender;
use tokio::runtime::Handle;
use zbus::zvariant::OwnedValue;

use crate::ask::{Input, Prompt, Reply, Source};
use crate::dialog::{ACCOUNT_MAX, BODY_MAX, DETAIL_MAX, Note, Response, TITLE_MAX};
use crate::queue::Queue;

pub struct Conversation {
    pub account: usize,
    pub prompt: Option<(String, bool)>,
    pub note: Option<Note>,
    pub busy: bool,
    pub attempt: Option<tokio::task::AbortHandle>,
    pub responder: Option<tokio::sync::mpsc::Sender<crate::ask::Secret>>,
}

pub struct Driver {
    pub conversation: Option<Conversation>,
    epoch: u64,
    runtime: Handle,
    input: Sender<Input>,
    socket: PathBuf,
    setuid: Vec<PathBuf>,
    cancelled: Arc<Mutex<HashSet<String>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEffect {
    None,
    Finish(Outcome),
    Restart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseEffect {
    Cancel,
    Submit,
    Restart { account: usize },
    Reconnect,
    Ignore,
}

pub fn detail_rows(details: HashMap<String, String>) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = details
        .into_iter()
        .filter(|(key, _)| !key.starts_with("polkit."))
        .filter_map(|(key, value)| {
            let key = clean(&key, ACCOUNT_MAX);
            let value = clean(&value, DETAIL_MAX);
            (!key.is_empty()).then_some((key, value))
        })
        .collect();
    rows.sort_by(|left, right| left.0.cmp(&right.0));
    rows
}

fn command_line(bytes: &[u8]) -> String {
    let command = String::from_utf8_lossy(bytes);
    clean(
        &command.trim_end_matches('\0').replace('\0', " "),
        DETAIL_MAX,
    )
}

pub async fn request(
    action_id: &str,
    message: &str,
    details: HashMap<String, String>,
    cookie: String,
    identities: Vec<Identity>,
) -> Request {
    let command = async {
        let pid = details
            .get("polkit.subject-pid")
            .or_else(|| details.get("polkit.caller-pid"))?
            .parse::<u32>()
            .ok()?;
        let bytes = tokio::fs::read(format!("/proc/{pid}/cmdline")).await.ok()?;
        let command = command_line(&bytes);
        (!command.is_empty()).then_some(command)
    }
    .await;
    let mut details = detail_rows(details);
    if let Some(command) = command {
        details.insert(0, (gettext("Command"), command));
    }
    Request {
        action_id: clean(action_id, DETAIL_MAX),
        message: clean(message, BODY_MAX),
        details,
        cookie,
        identities,
    }
}

pub fn unix_users(identities: &[(String, HashMap<String, OwnedValue>)]) -> Vec<u32> {
    identities
        .iter()
        .filter_map(|(kind, details)| {
            (kind == "unix-user")
                .then(|| uid(details.get("uid")?))
                .flatten()
        })
        .collect()
}

fn uid(value: &OwnedValue) -> Option<u32> {
    u32::try_from(&**value).ok().or_else(|| {
        i32::try_from(&**value)
            .ok()
            .and_then(|value| u32::try_from(value).ok())
    })
}

pub fn resolve_identities(
    uids: &[u32],
    passwd: &str,
    real_names: &HashMap<u32, String>,
    session_uid: u32,
) -> Vec<Identity> {
    let mut identities = Vec::new();
    for uid in uids {
        let Some((user, gecos)) = passwd_entry(passwd, *uid) else {
            continue;
        };
        let real = real_names
            .get(uid)
            .map(|name| clean(name, ACCOUNT_MAX))
            .filter(|name| !name.is_empty());
        let name = real.unwrap_or_else(|| clean(&gecos, ACCOUNT_MAX));
        identities.push(Identity {
            uid: *uid,
            user,
            name,
        });
    }
    if let Some(index) = identities
        .iter()
        .position(|identity| identity.uid == session_uid)
    {
        let session = identities.remove(index);
        identities.insert(0, session);
    }
    identities
}

fn passwd_entry(passwd: &str, uid: u32) -> Option<(String, String)> {
    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');
        let name = fields.next()?;
        let _password = fields.next()?;
        let entry = fields.next()?.parse::<u32>().ok()?;
        let _gid = fields.next()?;
        let gecos = fields.next().unwrap_or("");
        if entry != uid || name.is_empty() {
            return None;
        }
        let display = gecos.split(',').next().unwrap_or("").trim().to_owned();
        Some((name.to_owned(), display))
    })
}

pub fn on_helper_line(conversation: &mut Conversation, line: helper::Line) -> LineEffect {
    match line {
        helper::Line::Prompt { text, echo } => {
            conversation.prompt = Some((clean(&text, TITLE_MAX), echo));
            conversation.busy = false;
            LineEffect::None
        }
        helper::Line::Info(text) => {
            conversation.note = Some(Note::Info(clean(&text, BODY_MAX)));
            LineEffect::None
        }
        helper::Line::Error(text) => {
            conversation.note = Some(Note::Error(clean(&text, BODY_MAX)));
            LineEffect::None
        }
        helper::Line::Success => LineEffect::Finish(Outcome::Authorized),
        helper::Line::Failure => {
            conversation.note = Some(Note::Error(gettext("Wrong password")));
            conversation.busy = false;
            conversation.prompt = None;
            LineEffect::Restart
        }
        helper::Line::Lost => {
            let before_prompt = conversation.prompt.is_none();
            conversation.responder = None;
            conversation.attempt = None;
            conversation.busy = false;
            conversation.note = Some(match before_prompt {
                true => Note::Unavailable(gettext("Can't verify passwords")),
                false => Note::Error(gettext("Can't verify passwords")),
            });
            LineEffect::None
        }
    }
}

pub fn on_dialog(conversation: Option<&Conversation>, response: &Response) -> ResponseEffect {
    let account = match response {
        Response::Cancel => return ResponseEffect::Cancel,
        Response::Account(account) => {
            if conversation.is_some_and(|conversation| conversation.account == *account) {
                return ResponseEffect::Ignore;
            }
            return ResponseEffect::Restart { account: *account };
        }
        Response::Confirm { account, .. } => account,
    };
    let Some(conversation) = conversation else {
        return ResponseEffect::Restart { account: *account };
    };
    if *account != conversation.account || conversation.busy {
        return ResponseEffect::Ignore;
    }
    if matches!(conversation.note, Some(Note::Unavailable(_))) {
        return ResponseEffect::Ignore;
    }
    if conversation
        .responder
        .as_ref()
        .is_some_and(|responder| !responder.is_closed())
    {
        return ResponseEffect::Submit;
    }
    ResponseEffect::Reconnect
}

impl Driver {
    pub fn new(runtime: Handle, input: Sender<Input>) -> Self {
        let setuid = helper::SETUID.iter().map(PathBuf::from).collect();
        Self::with_helpers(runtime, input, PathBuf::from(helper::SOCKET), setuid)
    }

    pub fn with_helpers(
        runtime: Handle,
        input: Sender<Input>,
        socket: PathBuf,
        setuid: Vec<PathBuf>,
    ) -> Self {
        Self::with_cancelled(
            runtime,
            input,
            socket,
            setuid,
            Arc::new(Mutex::new(HashSet::new())),
        )
    }

    pub fn with_cancelled(
        runtime: Handle,
        input: Sender<Input>,
        socket: PathBuf,
        setuid: Vec<PathBuf>,
        cancelled: Arc<Mutex<HashSet<String>>>,
    ) -> Self {
        Self {
            conversation: None,
            epoch: 0,
            runtime,
            input,
            socket,
            setuid,
            cancelled,
        }
    }

    pub fn start(&mut self, queue: &Queue) {
        self.launch(queue, 0, None);
    }

    pub fn stop(&mut self) {
        if let Some(attempt) = self
            .conversation
            .take()
            .and_then(|conversation| conversation.attempt)
        {
            attempt.abort();
        }
    }

    pub fn on_line(&mut self, queue: &mut Queue, key: &str, line: helper::Line) {
        let Some((epoch, cookie)) = helper::split_tagged_key(key) else {
            return;
        };
        let current = queue
            .shown()
            .is_some_and(|ask| ask.source == Source::Polkit && ask.key == cookie);
        let Some(conversation) = self.conversation.as_mut() else {
            return;
        };
        if !current || epoch != self.epoch {
            return;
        }
        match on_helper_line(conversation, line) {
            LineEffect::None => {}
            LineEffect::Finish(outcome) => self.finish(queue, outcome),
            LineEffect::Restart => self.relaunch(queue),
        }
    }

    pub fn on_response(&mut self, queue: &mut Queue, response: Response) {
        match on_dialog(self.conversation.as_ref(), &response) {
            ResponseEffect::Cancel => self.finish(queue, Outcome::Cancelled),
            ResponseEffect::Submit => {
                let Response::Confirm { secret, .. } = response else {
                    return;
                };
                if let Some(conversation) = self.conversation.as_mut() {
                    conversation.busy = true;
                    if let Some(responder) = &conversation.responder {
                        let _ = responder.try_send(secret);
                    }
                }
            }
            ResponseEffect::Restart { account } => self.launch(queue, account, None),
            ResponseEffect::Reconnect => self.relaunch(queue),
            ResponseEffect::Ignore => {}
        }
    }

    fn relaunch(&mut self, queue: &Queue) {
        let (account, note) = self
            .conversation
            .as_ref()
            .map_or((0, None), |conversation| {
                (conversation.account, conversation.note.clone())
            });
        self.launch(queue, account, note);
    }

    fn launch(&mut self, queue: &Queue, account: usize, note: Option<Note>) {
        self.stop();
        self.epoch = self.epoch.wrapping_add(1);
        let Some(ask) = queue.shown() else {
            return;
        };
        let Prompt::Polkit(request) = &ask.prompt else {
            return;
        };
        if self
            .cancelled
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains(&request.cookie)
        {
            return;
        }
        let Some(identity) = request.identities.get(account) else {
            self.conversation = Some(Conversation {
                account,
                prompt: None,
                note: Some(Note::Error(gettext("Can't verify passwords"))),
                busy: true,
                attempt: None,
                responder: None,
            });
            return;
        };
        let (responder, secrets) = tokio::sync::mpsc::channel(1);
        let attempt = self.runtime.spawn(helper::drive(
            identity.user.clone(),
            request.cookie.clone(),
            self.socket.clone(),
            self.setuid.clone(),
            helper::tagged_key(&ask.key, self.epoch),
            self.input.clone(),
            secrets,
        ));
        self.conversation = Some(Conversation {
            account,
            prompt: None,
            note,
            busy: false,
            attempt: Some(attempt.abort_handle()),
            responder: Some(responder),
        });
    }

    fn finish(&mut self, queue: &mut Queue, outcome: Outcome) {
        if let Some(ask) = queue.take_head()
            && let Reply::Polkit(reply) = ask.reply
        {
            let _ = reply.send(outcome);
        }
        self.stop();
    }
}

#[cfg(test)]
mod tests;
