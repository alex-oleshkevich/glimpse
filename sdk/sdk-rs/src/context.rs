use crate::protocol::{In, MAX_LINE};
use crate::widget::Widget;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;

#[derive(Clone)]
pub struct WakeHandle {
    pub(crate) tx: Sender<In>,
}

impl WakeHandle {
    pub fn wake(&self) {
        let _ = self.tx.send(In::Wake);
    }
}

pub type Waker = WakeHandle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppletEntry {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
}

pub struct Ctx {
    pub name: String,
    pub state_dir: String,
    pub options: Value,
    pub placement: Option<Value>,
    pub popover_open: bool,
    pub entry: Option<AppletEntry>,
    pub(crate) tx: Sender<In>,
}

impl Ctx {
    pub fn wake_handle(&self) -> WakeHandle {
        WakeHandle {
            tx: self.tx.clone(),
        }
    }

    pub fn waker(&self) -> Waker {
        self.wake_handle()
    }

    pub fn wake(&self) {
        let _ = self.tx.send(In::Wake);
    }

    fn state_file(&self) -> Option<PathBuf> {
        if self.state_dir.is_empty() {
            None
        } else {
            Some(PathBuf::from(&self.state_dir).join("state.json"))
        }
    }

    pub fn load_state<S: DeserializeOwned>(&self) -> Option<S> {
        let text = std::fs::read_to_string(self.state_file()?).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn save_state<S: Serialize>(&self, state: &S) -> std::io::Result<()> {
        let Some(path) = self.state_file() else {
            return Ok(());
        };
        let text = serde_json::to_string(state).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    fn send(&self, value: Value) {
        if let Ok(line) = serde_json::to_string(&value)
            && line.len() <= MAX_LINE
        {
            let _ = self.tx.send(In::Send(line));
        }
    }

    pub fn notify(
        &self,
        summary: &str,
        body: Option<&str>,
        icon: Option<&str>,
        urgency: Option<&str>,
    ) {
        let args = notify_argv(
            &self.name,
            self.entry.as_ref(),
            summary,
            body,
            icon,
            urgency,
        );
        spawn("notify-send", &args, None);
    }

    pub fn copy(&self, text: &str) {
        spawn("wl-copy", &[], Some(text));
    }

    pub fn open_uri(&self, uri: &str) {
        self.send(json!({"t": "open-uri", "uri": uri}));
    }

    pub fn launch(&self, id: &str) {
        self.send(json!({"t": "launch", "id": id}));
    }

    pub fn close_popover(&self) {
        self.send(json!({"t": "close-popover"}));
    }
}

pub(crate) fn notify_argv(
    name: &str,
    entry: Option<&AppletEntry>,
    summary: &str,
    body: Option<&str>,
    icon: Option<&str>,
    urgency: Option<&str>,
) -> Vec<String> {
    let app = entry
        .map(|entry| entry.name.as_str())
        .filter(|app| !app.is_empty())
        .unwrap_or(name);
    let urgency = urgency
        .filter(|level| matches!(*level, "low" | "normal" | "critical"))
        .unwrap_or("normal");
    let mut args = vec![format!("--app-name={app}"), format!("--urgency={urgency}")];
    let icon = icon.filter(|icon| !icon.is_empty()).or_else(|| {
        entry
            .and_then(|entry| entry.icon.as_deref())
            .filter(|icon| !icon.is_empty())
    });
    if let Some(icon) = icon {
        args.push(format!("--icon={icon}"));
    }
    if let Some(entry) = entry.filter(|entry| !entry.id.is_empty()) {
        let id = entry.id.strip_suffix(".desktop").unwrap_or(&entry.id);
        args.push(format!("--hint=string:desktop-entry:{id}"));
    }
    args.push("--".to_owned());
    args.push(summary.to_owned());
    if let Some(body) = body.filter(|body| !body.is_empty()) {
        args.push(body.to_owned());
    }
    args
}

pub(crate) fn spawn(program: &str, args: &[String], stdin: Option<&str>) {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            eprintln!("cannot start {program}: {err}");
            return;
        }
    };
    let pipe = child.stdin.take();
    let text = stdin.map(str::to_owned);
    std::thread::spawn(move || {
        if let (Some(text), Some(mut pipe)) = (text, pipe) {
            let _ = pipe.write_all(text.as_bytes());
        }
        let _ = child.wait();
    });
}

pub trait App {
    type Msg;

    fn start(&mut self, _ctx: &Ctx) {}

    fn update(&mut self, ctx: &Ctx, msg: Self::Msg);
    fn build(&self, ctx: &Ctx) -> Vec<Widget<Self::Msg>>;
}
