use async_trait::async_trait;
use glimpse_sdk::{
    ActionItem, Applet, AppletResult, Column, EmptyState, Hero, Item, PopoverScaffold,
    Separator, StatusItem, Switch, TreeNode, ipc, mpsc, run,
};
use serde_json::Value;
use tokio::{
    io::{AsyncWriteExt, BufReader, AsyncBufReadExt},
    net::UnixStream,
    process::Command,
};

#[derive(Debug, Clone, PartialEq)]
struct Monitor {
    name: String,
    description: Option<String>,
    make: Option<String>,
    model: Option<String>,
    enabled: bool,
    built_in: bool,
    width: Option<u32>,
    height: Option<u32>,
    refresh_hz: Option<f64>,
    // Retained for Hyprland re-enable
    x: Option<i32>,
    y: Option<i32>,
    scale: Option<f64>,
    transform: Option<u32>,
}

impl Monitor {
    fn display_name(&self) -> &str {
        self.description
            .as_deref()
            .or(self.model.as_deref())
            .unwrap_or(&self.name)
    }

    fn resolution_label(&self) -> String {
        match (self.width, self.height, self.refresh_hz) {
            (Some(w), Some(h), Some(r)) => format!("{w}×{h} @ {r:.0} Hz"),
            (Some(w), Some(h), None) => format!("{w}×{h}"),
            _ => "Disabled".to_owned(),
        }
    }

    fn hyprland_enable_args(&self) -> String {
        let name = &self.name;
        match (self.width, self.height, self.refresh_hz) {
            (Some(w), Some(h), Some(r)) => {
                let pos = format!("{}x{}", self.x.unwrap_or(0), self.y.unwrap_or(0));
                let scale = self.scale.unwrap_or(1.0);
                let mut args = format!("{name},{w}x{h}@{r:.3},{pos},{scale}");
                if let Some(t) = self.transform
                    && t != 0
                {
                    args.push_str(&format!(",transform,{t}"));
                }
                args
            }
            _ => format!("{name},preferred,auto,1"),
        }
    }
}

#[derive(Debug, Clone)]
struct State {
    monitors: Vec<Monitor>,
    keep_one: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            monitors: Vec::new(),
            keep_one: true,
        }
    }
}

impl State {
    fn enabled_count(&self) -> usize {
        self.monitors.iter().filter(|m| m.enabled).count()
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Reload,
    SetEnabled(String, bool),
    SetKeepOne(bool),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Compositor {
    Hyprland,
    Niri,
}

impl Compositor {
    fn detect() -> Option<Self> {
        if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
            Some(Self::Hyprland)
        } else if std::env::var("NIRI_SOCKET").is_ok() {
            Some(Self::Niri)
        } else {
            None
        }
    }

    async fn fetch_monitors(self) -> AppletResult<Vec<Monitor>> {
        match self {
            Self::Hyprland => fetch_hyprland_monitors().await,
            Self::Niri => fetch_niri_monitors().await,
        }
    }

    async fn set_monitor_enabled(self, monitor: &Monitor, on: bool) -> AppletResult<()> {
        match self {
            Self::Hyprland => hyprland_set_enabled(monitor, on).await,
            Self::Niri => niri_set_enabled(&monitor.name, on).await,
        }
    }
}

// ── Hyprland ─────────────────────────────────────────────────────────────────

async fn fetch_hyprland_monitors() -> AppletResult<Vec<Monitor>> {
    let output = Command::new("hyprctl")
        .args(["monitors", "-j"])
        .output()
        .await?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    let value: Value = serde_json::from_slice(&output.stdout)?;
    Ok(parse_hyprland_monitors(&value))
}

fn parse_hyprland_monitors(value: &Value) -> Vec<Monitor> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let name = m.get("name")?.as_str()?.to_owned();
            let disabled = m.get("disabled").and_then(Value::as_bool).unwrap_or(false);
            let enabled = !disabled;
            let (width, height, refresh_hz) = if enabled {
                (
                    m.get("width").and_then(Value::as_u64).map(|v| v as u32),
                    m.get("height").and_then(Value::as_u64).map(|v| v as u32),
                    m.get("refreshRate").and_then(Value::as_f64),
                )
            } else {
                (None, None, None)
            };
            Some(Monitor {
                built_in: is_builtin(&name),
                name,
                description: m.get("description").and_then(Value::as_str).map(str::to_owned),
                make: m.get("make").and_then(Value::as_str).map(str::to_owned),
                model: m.get("model").and_then(Value::as_str).map(str::to_owned),
                enabled,
                width,
                height,
                refresh_hz,
                x: m.get("x").and_then(Value::as_i64).map(|v| v as i32),
                y: m.get("y").and_then(Value::as_i64).map(|v| v as i32),
                scale: m.get("scale").and_then(Value::as_f64),
                transform: m.get("transform").and_then(Value::as_u64).map(|v| v as u32),
            })
        })
        .collect()
}

async fn hyprland_set_enabled(monitor: &Monitor, on: bool) -> AppletResult<()> {
    let args = if on {
        monitor.hyprland_enable_args()
    } else {
        format!("{},disable", monitor.name)
    };
    let output = Command::new("hyprctl")
        .args(["keyword", "monitor", &args])
        .output()
        .await?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("hyprctl failed: {err}").into());
    }
    Ok(())
}

// ── Niri ──────────────────────────────────────────────────────────────────────

async fn fetch_niri_monitors() -> AppletResult<Vec<Monitor>> {
    let output = Command::new("niri")
        .args(["msg", "--json", "outputs"])
        .output()
        .await?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    let value: Value = serde_json::from_slice(&output.stdout)?;
    Ok(parse_niri_monitors(&value))
}

fn parse_niri_monitors(value: &Value) -> Vec<Monitor> {
    let Some(outputs) = value.as_object() else {
        return Vec::new();
    };
    outputs
        .iter()
        .map(|(key, output)| {
            let name = output
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(key)
                .to_owned();
            let current_mode_idx = output
                .get("current_mode")
                .and_then(|v| if v.is_null() { None } else { v.as_u64() });
            let enabled = current_mode_idx.is_some();
            let (width, height, refresh_hz) = current_mode_idx
                .and_then(|idx| {
                    let mode = output.get("modes")?.as_array()?.get(idx as usize)?;
                    Some((
                        mode.get("width").and_then(Value::as_u64).map(|v| v as u32),
                        mode.get("height").and_then(Value::as_u64).map(|v| v as u32),
                        mode.get("refresh_rate")
                            .and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|f| f as u64)))
                            .map(|mhz| mhz as f64 / 1000.0),
                    ))
                })
                .unwrap_or((None, None, None));
            Monitor {
                built_in: is_builtin(&name),
                name,
                description: output.get("description").and_then(Value::as_str).map(str::to_owned),
                make: output.get("make").and_then(Value::as_str).map(str::to_owned),
                model: output.get("model").and_then(Value::as_str).map(str::to_owned),
                enabled,
                width,
                height,
                refresh_hz,
                x: None,
                y: None,
                scale: None,
                transform: None,
            }
        })
        .collect()
}

async fn niri_set_enabled(name: &str, on: bool) -> AppletResult<()> {
    let socket_path = std::env::var("NIRI_SOCKET")
        .map_err(|_| "NIRI_SOCKET not set")?;
    let action = if on { "On" } else { "Off" };
    let request = serde_json::to_vec(&serde_json::json!({
        "Output": { "output": name, "action": action }
    }))?;
    let mut stream = UnixStream::connect(&socket_path).await
        .map_err(|e| format!("niri socket: {e}"))?;
    stream.write_all(&request).await?;
    stream.write_all(b"\n").await?;
    stream.shutdown().await?;
    let mut lines = BufReader::new(stream).lines();
    match lines.next_line().await? {
        Some(reply) if reply.contains("\"Ok\"") => Ok(()),
        Some(reply) => Err(format!("niri: {reply}").into()),
        None => Err("niri: connection closed without reply".into()),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn is_builtin(name: &str) -> bool {
    name.starts_with("eDP") || name.starts_with("LVDS") || name.starts_with("DSI")
}

// ── Applet ────────────────────────────────────────────────────────────────────

struct MonitorsApplet {
    compositor: Option<Compositor>,
}

#[async_trait]
impl Applet for MonitorsApplet {
    type State = State;
    type Msg = Msg;

    async fn on_start(&mut self, state: &mut State, tx: mpsc::Sender<Msg>) -> AppletResult<()> {
        if let Some(compositor) = self.compositor {
            match compositor.fetch_monitors().await {
                Ok(m) => state.monitors = m,
                Err(e) => self.log(format!("monitors: initial fetch failed: {e}")),
            }
        }

        tokio::spawn(async move {
            let Ok(subscriber) = ipc("shell") else { return };
            let Ok(mut stream) = subscriber.listen("monitor.*").await else { return };
            while let Some(Ok(_)) = stream.next().await {
                if tx.send(Msg::Reload).await.is_err() {
                    break;
                }
            }
        });

        Ok(())
    }

    async fn update(&mut self, state: &mut State, msg: Msg) -> AppletResult<()> {
        match msg {
            Msg::Reload => {
                if let Some(compositor) = self.compositor {
                    match compositor.fetch_monitors().await {
                        Ok(m) => state.monitors = m,
                        Err(e) => self.log(format!("monitors: reload failed: {e}")),
                    }
                }
            }
            Msg::SetEnabled(name, on) => {
                if state.keep_one && !on && state.enabled_count() <= 1 {
                    return Ok(());
                }
                if let Some(compositor) = self.compositor {
                    if let Some(monitor) = state.monitors.iter().find(|m| m.name == name) {
                        let monitor = monitor.clone();
                        if let Err(e) = compositor.set_monitor_enabled(&monitor, on).await {
                            self.log(format!("monitors: set_enabled failed: {e}"));
                            return Ok(());
                        }
                    }
                }
                if let Some(m) = state.monitors.iter_mut().find(|m| m.name == name) {
                    m.enabled = on;
                    if !on {
                        m.width = None;
                        m.height = None;
                        m.refresh_hz = None;
                    }
                }
            }
            Msg::SetKeepOne(v) => state.keep_one = v,
        }
        Ok(())
    }

    async fn status(&self, _state: &State) -> AppletResult<Vec<StatusItem>> {
        Ok(vec![StatusItem::new("monitors").icon("video-display-symbolic")])
    }

    async fn popover(&self, state: &State) -> AppletResult<Option<TreeNode<Msg>>> {
        if state.monitors.is_empty() {
            return Ok(Some(
                EmptyState::new("No monitors")
                    .subtitle("No monitors detected")
                    .into(),
            ));
        }

        let enabled_count = state.enabled_count();

        let mut rows: Vec<TreeNode<Msg>> = state
            .monitors
            .iter()
            .map(|m| {
                let icon = if m.built_in {
                    "computer-symbolic"
                } else {
                    "video-display-symbolic"
                };
                let is_last_enabled = state.keep_one && m.enabled && enabled_count == 1;
                let name = m.name.clone();
                let switch = Switch::new(format!("toggle_{}", m.name))
                    .active(m.enabled);
                if is_last_enabled {
                    ActionItem::new(format!("monitor_{}", m.name), m.display_name())
                        .icon(icon)
                        .sublabel(format!("{} · {}", m.name, m.resolution_label()))
                        .right(switch)
                        .enabled(false)
                        .into()
                } else {
                    Item::new(m.display_name())
                        .icon(icon)
                        .sublabel(format!("{} · {}", m.name, m.resolution_label()))
                        .right(switch.on_toggle(move |v| Msg::SetEnabled(name.clone(), v)))
                        .into()
                }
            })
            .collect();

        rows.push(Separator::new().into());
        rows.push(
            ActionItem::new("keep_one", "Keep at least one monitor")
                .right(
                    Switch::new("toggle_keep_one")
                        .active(state.keep_one)
                        .on_toggle(Msg::SetKeepOne),
                )
                .into(),
        );

        let count = state.monitors.len();
        let subtitle = format!("{count} display{}", if count == 1 { "" } else { "s" });

        Ok(Some(
            PopoverScaffold::new(Column::new(rows).spacing(0))
                .hero(Hero::new("Monitors", subtitle).icon("video-display-symbolic"))
                .into(),
        ))
    }
}

#[tokio::main]
async fn main() -> AppletResult<()> {
    run(
        MonitorsApplet {
            compositor: Compositor::detect(),
        },
        State::default(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_hyprland_monitors_enabled() {
        let value = json!([{
            "name": "eDP-1",
            "description": "AU Optronics eDP-1",
            "make": "AU Optronics",
            "model": "AUO B156HAN",
            "width": 1920,
            "height": 1080,
            "refreshRate": 60.0,
            "x": 0,
            "y": 0,
            "scale": 1.0,
            "transform": 0,
            "disabled": false
        }]);
        let monitors = parse_hyprland_monitors(&value);
        assert_eq!(monitors.len(), 1);
        let m = &monitors[0];
        assert_eq!(m.name, "eDP-1");
        assert!(m.built_in);
        assert!(m.enabled);
        assert_eq!(m.width, Some(1920));
        assert_eq!(m.height, Some(1080));
        assert_eq!(m.refresh_hz, Some(60.0));
        assert_eq!(m.x, Some(0));
        assert_eq!(m.scale, Some(1.0));
    }

    #[test]
    fn parse_hyprland_monitors_disabled_has_no_mode() {
        let value = json!([{
            "name": "HDMI-A-1",
            "description": "Some Monitor",
            "make": null,
            "model": null,
            "width": 0,
            "height": 0,
            "refreshRate": 0.0,
            "disabled": true
        }]);
        let monitors = parse_hyprland_monitors(&value);
        assert_eq!(monitors.len(), 1);
        let m = &monitors[0];
        assert!(!m.enabled);
        assert!(!m.built_in);
        assert_eq!(m.width, None);
        assert_eq!(m.height, None);
    }

    #[test]
    fn parse_niri_monitors_enabled() {
        let value = json!({
            "eDP-1": {
                "name": "eDP-1",
                "make": "AU Optronics",
                "model": "eDP-1",
                "current_mode": 0,
                "modes": [{"width": 2560, "height": 1600, "refresh_rate": 165000, "is_preferred": true}]
            }
        });
        let monitors = parse_niri_monitors(&value);
        assert_eq!(monitors.len(), 1);
        let m = &monitors[0];
        assert!(m.enabled);
        assert!(m.built_in);
        assert_eq!(m.width, Some(2560));
        assert_eq!(m.height, Some(1600));
        assert_eq!(m.refresh_hz, Some(165.0));
    }

    #[test]
    fn parse_niri_monitors_null_current_mode_is_disabled() {
        let value = json!({
            "HDMI-A-1": {
                "name": "HDMI-A-1",
                "description": null,
                "make": null,
                "model": null,
                "current_mode": null,
                "modes": [{"width": 1920, "height": 1080, "refresh_rate": 60000}]
            }
        });
        let monitors = parse_niri_monitors(&value);
        assert_eq!(monitors.len(), 1);
        assert!(!monitors[0].enabled);
        assert_eq!(monitors[0].width, None);
    }

    #[test]
    fn parse_niri_monitors_float_refresh_rate() {
        let value = json!({
            "DP-1": {
                "name": "DP-1",
                "description": null,
                "make": null,
                "model": null,
                "current_mode": 0,
                "modes": [{"width": 1920, "height": 1080, "refresh_rate": 60000.0}]
            }
        });
        let monitors = parse_niri_monitors(&value);
        assert_eq!(monitors[0].refresh_hz, Some(60.0));
    }

    #[test]
    fn resolution_label_formats_correctly() {
        let m = Monitor {
            name: "eDP-1".into(),
            description: None,
            make: None,
            model: None,
            enabled: true,
            built_in: true,
            width: Some(1920),
            height: Some(1080),
            refresh_hz: Some(60.0),
            x: None, y: None, scale: None, transform: None,
        };
        assert_eq!(m.resolution_label(), "1920×1080 @ 60 Hz");
    }

    #[test]
    fn display_name_prefers_description_over_model_over_name() {
        let mut m = Monitor {
            name: "eDP-1".into(),
            description: Some("Built-in Display".into()),
            make: None,
            model: Some("AUO".into()),
            enabled: true,
            built_in: true,
            width: None, height: None, refresh_hz: None,
            x: None, y: None, scale: None, transform: None,
        };
        assert_eq!(m.display_name(), "Built-in Display");
        m.description = None;
        assert_eq!(m.display_name(), "AUO");
        m.model = None;
        assert_eq!(m.display_name(), "eDP-1");
    }

    #[test]
    fn hyprland_enable_args_uses_stored_config() {
        let m = Monitor {
            name: "HDMI-A-1".into(),
            description: None, make: None, model: None,
            enabled: false, built_in: false,
            width: Some(1920), height: Some(1080), refresh_hz: Some(60.0),
            x: Some(1920), y: Some(0), scale: Some(1.0), transform: Some(0),
        };
        assert_eq!(m.hyprland_enable_args(), "HDMI-A-1,1920x1080@60.000,1920x0,1");
    }

    #[test]
    fn hyprland_enable_args_falls_back_without_config() {
        let m = Monitor {
            name: "HDMI-A-1".into(),
            description: None, make: None, model: None,
            enabled: false, built_in: false,
            width: None, height: None, refresh_hz: None,
            x: None, y: None, scale: None, transform: None,
        };
        assert_eq!(m.hyprland_enable_args(), "HDMI-A-1,preferred,auto,1");
    }

    #[test]
    fn keep_one_blocks_last_monitor_disable() {
        let state = State {
            monitors: vec![Monitor {
                name: "eDP-1".into(),
                description: None, make: None, model: None,
                enabled: true, built_in: true,
                width: Some(1920), height: Some(1080), refresh_hz: Some(60.0),
                x: None, y: None, scale: None, transform: None,
            }],
            keep_one: true,
        };
        assert_eq!(state.enabled_count(), 1);
        // With keep_one=true and only 1 enabled, the toggle should be blocked
        assert!(state.keep_one && state.enabled_count() == 1);
    }
}
