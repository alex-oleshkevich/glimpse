use crate::protocol::{In, MAX_LINE};
use crate::widget::Widget;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn position_now(player: &Value) -> f64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as f64)
        .unwrap_or(0.0);
    position_now_at(player, now)
}

pub fn position_now_at(player: &Value, now_ms: f64) -> f64 {
    let position = player["positionUs"].as_f64().unwrap_or(0.0);
    if player["playback"].as_str() != Some("playing") {
        return position;
    }
    let at = player["positionAt"].as_f64().unwrap_or(0.0);
    let rate = player["rate"].as_f64().unwrap_or(1.0);
    let out = position + (now_ms - at) * 1000.0 * rate;
    match player["lengthUs"].as_f64() {
        Some(length) if length > 0.0 => out.min(length),
        _ => out,
    }
}

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

pub struct Ctx {
    pub name: String,
    pub state_dir: String,
    pub options: Value,
    pub placement: Option<Value>,
    pub popover_open: bool,
    pub services: Map<String, Value>,
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

    pub fn mpris(&self) -> Option<Vec<Value>> {
        self.services
            .get("mpris")?
            .get("players")?
            .as_array()
            .cloned()
    }

    pub fn compositor(&self) -> Option<&Value> {
        self.services.get("compositor")
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
        let mut msg = json!({"t": "notify", "summary": summary});
        if let Some(fields) = msg.as_object_mut() {
            if let Some(body) = body {
                fields.insert("body".to_owned(), json!(body));
            }
            if let Some(icon) = icon {
                fields.insert("icon".to_owned(), json!(icon));
            }
            if let Some(urgency) = urgency {
                fields.insert("urgency".to_owned(), json!(urgency));
            }
        }
        self.send(msg);
    }

    pub fn copy(&self, text: &str) {
        self.send(json!({"t": "copy", "text": text}));
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

    pub fn mpris_control(&self, player: &str, action: &str) {
        self.send(json!({"t": "mpris", "player": player, "op": "control", "action": action}));
    }

    pub fn mpris_seek(&self, player: &str, offset_us: i64) {
        self.send(json!({"t": "mpris", "player": player, "op": "seek", "offset": offset_us}));
    }

    pub fn mpris_set_position(&self, player: &str, position_us: i64) {
        self.send(
            json!({"t": "mpris", "player": player, "op": "set-position", "position": position_us}),
        );
    }

    pub fn focus_workspace(&self, target: Value) {
        self.send(json!({"t": "compositor", "op": "focus-workspace", "target": target}));
    }

    pub fn focus_window(&self, target: Value) {
        self.send(json!({"t": "compositor", "op": "focus-window", "target": target}));
    }

    pub fn move_window(&self, window: i64, target: Value) {
        self.send(
            json!({"t": "compositor", "op": "move-window", "window": window, "target": target}),
        );
    }

    pub fn rename_workspace(&self, id: i64, name: Option<&str>) {
        self.send(json!({"t": "compositor", "op": "rename-workspace", "id": id, "name": name}));
    }

    pub fn close_window(&self, id: i64) {
        self.send(json!({"t": "compositor", "op": "close-window", "id": id}));
    }
}

pub trait App {
    type Msg;

    fn start(&mut self, _ctx: &Ctx) {}

    fn update(&mut self, ctx: &Ctx, msg: Self::Msg);
    fn build(&self, ctx: &Ctx) -> Vec<Widget<Self::Msg>>;
}
