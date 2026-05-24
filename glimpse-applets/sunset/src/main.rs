use std::collections::HashMap;

use async_trait::async_trait;
use chrono::NaiveTime;
use glimpse_sdk::{
    Applet, AppletResult, BoxedList, Choice, ChoiceList, EmptyState, ExpanderTile, Hero, Label,
    MsgMapper, PopoverShell, PopoverSize, SliderTile, StatusItem, SwitchTile, TreeNode, ipc, run,
    tree,
};

const DAYLIGHT_KELVIN: u32 = 6500;

#[derive(Debug, Clone, PartialEq, Eq)]
struct SunsetState {
    phase: String,
    health: String,
    schedule: String,
    effective_kelvin: u32,
    target_kelvin: u32,
    next_sunset: Option<String>,
    last_error: Option<String>,
}

impl Default for SunsetState {
    fn default() -> Self {
        Self {
            phase: "unknown".into(),
            health: "starting".into(),
            schedule: "off".into(),
            effective_kelvin: DAYLIGHT_KELVIN,
            target_kelvin: DAYLIGHT_KELVIN,
            next_sunset: None,
            last_error: None,
        }
    }
}

impl SunsetState {
    fn active(&self) -> bool {
        self.effective_kelvin < DAYLIGHT_KELVIN
            || matches!(self.phase.as_str(), "night" | "transition_to_night")
    }

    fn sunset_socket_missing(&self) -> bool {
        self.last_error.as_deref().is_some_and(|error| {
            error.contains("cannot connect to IPC socket")
                && (error.contains("No such file or directory") || error.contains("os error 2"))
        })
    }

    fn apply_status_fields(&mut self, fields: HashMap<String, String>) {
        self.apply_string(&fields, "phase", |state, value| state.phase = value);
        self.apply_string(&fields, "health", |state, value| state.health = value);
        self.apply_string(&fields, "schedule", |state, value| state.schedule = value);
        self.apply_kelvin(&fields, "kelvin", |state, kelvin| {
            state.effective_kelvin = kelvin;
        });
        self.apply_kelvin(&fields, "target_kelvin", |state, kelvin| {
            state.target_kelvin = kelvin;
        });
        self.apply_string(&fields, "sunset", |state, value| {
            state.next_sunset = Some(value);
        });
        self.last_error = None;
    }

    fn apply_event(&mut self, name: &str, fields: HashMap<String, String>) {
        match name {
            "nightlight.phase_changed" => {
                self.apply_string(&fields, "phase", |state, value| state.phase = value);
            }
            "nightlight.activated" => {
                self.phase = "night".into();
                self.apply_kelvin(&fields, "temperature", |state, kelvin| {
                    state.effective_kelvin = kelvin;
                    state.target_kelvin = kelvin;
                });
            }
            "nightlight.deactivated" => {
                self.effective_kelvin = DAYLIGHT_KELVIN;
            }
            "nightlight.temperature_changed" => {
                self.apply_string(&fields, "phase", |state, value| state.phase = value);
                self.apply_kelvin(&fields, "kelvin", |state, kelvin| {
                    state.effective_kelvin = kelvin;
                    state.target_kelvin = kelvin;
                });
            }
            "nightlight.health_changed" => {
                self.apply_string(&fields, "health", |state, value| state.health = value);
            }
            _ => {}
        }
        self.last_error = None;
    }

    fn apply_string(
        &mut self,
        fields: &HashMap<String, String>,
        key: &str,
        apply: impl FnOnce(&mut Self, String),
    ) {
        if let Some(value) = fields.get(key) {
            apply(self, value.clone());
        }
    }

    fn apply_kelvin(
        &mut self,
        fields: &HashMap<String, String>,
        key: &str,
        apply: impl FnOnce(&mut Self, u32),
    ) {
        if let Some(value) = fields.get(key).and_then(|value| value.parse().ok()) {
            apply(self, value);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IpcCommandPlan {
    action: &'static str,
    params: Vec<(&'static str, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Msg {
    Status(HashMap<String, String>),
    Event {
        name: String,
        fields: HashMap<String, String>,
    },
    ToggleNightLight(bool),
    SetTemperature(u32),
    SetSchedule(String),
    Error(String),
}

struct SunsetApplet;

#[async_trait]
impl Applet for SunsetApplet {
    type State = SunsetState;
    type Msg = Msg;

    async fn status(&self, state: &SunsetState) -> AppletResult<Vec<StatusItem>> {
        Ok(vec![
            StatusItem::new("sunset")
                .icon(status_icon(state))
                .tooltip(format!("Night light: {}, {}", state.phase, state.health)),
        ])
    }

    async fn on_start(
        &mut self,
        _state: &mut SunsetState,
        tx: glimpse_sdk::mpsc::Sender<Msg>,
    ) -> AppletResult<()> {
        spawn_status_refresh(tx.clone());
        spawn_event_subscription(tx);
        Ok(())
    }

    async fn popover(&self, state: &SunsetState) -> AppletResult<Option<TreeNode<Msg>>> {
        Ok(Some(popover_tree(state)))
    }

    async fn update(&mut self, state: &mut SunsetState, msg: Msg) -> AppletResult<()> {
        match msg.clone() {
            Msg::Status(fields) => state.apply_status_fields(fields),
            Msg::Event { name, fields } => state.apply_event(&name, fields),
            Msg::Error(error) => state.last_error = Some(error),
            Msg::ToggleNightLight(enabled) => {
                dispatch_interactive_command(&msg, state).await;
                state.schedule = if enabled { "automatic" } else { "off" }.into();
                if enabled {
                    state.phase = "night".into();
                    state.effective_kelvin = state.target_kelvin;
                } else {
                    state.phase = "disabled".into();
                    state.effective_kelvin = DAYLIGHT_KELVIN;
                }
            }
            Msg::SetTemperature(kelvin) => {
                dispatch_interactive_command(&msg, state).await;
                state.target_kelvin = kelvin;
                if state.active() {
                    state.effective_kelvin = kelvin;
                }
            }
            Msg::SetSchedule(schedule) => {
                if let Some(schedule) = normalize_schedule(&schedule) {
                    dispatch_interactive_command(&msg, state).await;
                    state.schedule = schedule.into();
                    if schedule == "off" {
                        state.phase = "disabled".into();
                        state.effective_kelvin = DAYLIGHT_KELVIN;
                    } else if state.phase == "disabled" {
                        state.phase = "day".into();
                        state.effective_kelvin = DAYLIGHT_KELVIN;
                    }
                } else {
                    state.last_error = Some(format!("unknown night light schedule: {schedule}"));
                }
            }
        }
        Ok(())
    }
}

fn ipc_commands_for_msg(msg: &Msg) -> Vec<IpcCommandPlan> {
    match msg {
        Msg::ToggleNightLight(true) => vec![
            IpcCommandPlan {
                action: "enable",
                params: Vec::new(),
            },
            IpcCommandPlan {
                action: "activate",
                params: Vec::new(),
            },
        ],
        Msg::ToggleNightLight(false) => vec![IpcCommandPlan {
            action: "disable",
            params: Vec::new(),
        }],
        Msg::SetTemperature(kelvin) => vec![IpcCommandPlan {
            action: "set_temperature",
            params: vec![("kelvin", kelvin.to_string())],
        }],
        Msg::SetSchedule(schedule) => normalize_schedule(schedule)
            .map(|schedule| {
                vec![IpcCommandPlan {
                    action: "set_schedule",
                    params: vec![("schedule", schedule.to_string())],
                }]
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

async fn dispatch_interactive_command(msg: &Msg, state: &mut SunsetState) {
    let plans = ipc_commands_for_msg(msg);
    if plans.is_empty() {
        return;
    }
    match ipc("sunset") {
        Ok(subscriber) => {
            for plan in plans {
                if let Err(error) = subscriber.dispatch(plan.action, plan.params).await {
                    state.last_error = Some(format!("glimpse-sunset IPC failed: {error}"));
                    return;
                }
            }
            state.last_error = None;
        }
        Err(error) => state.last_error = Some(format!("glimpse-sunset IPC unavailable: {error}")),
    }
}

fn spawn_status_refresh(tx: glimpse_sdk::mpsc::Sender<Msg>) {
    tokio::spawn(async move {
        match ipc("sunset") {
            Ok(subscriber) => match subscriber
                .dispatch("status", Vec::<(&str, &str)>::new())
                .await
            {
                Ok(mut fields) => {
                    if let Ok(solar) = subscriber
                        .dispatch("solar", Vec::<(&str, &str)>::new())
                        .await
                    {
                        fields.extend(solar);
                    }
                    let _ = tx.send(Msg::Status(fields)).await;
                }
                Err(error) => {
                    let _ = tx
                        .send(Msg::Error(format!("glimpse-sunset status failed: {error}")))
                        .await;
                }
            },
            Err(error) => {
                let _ = tx
                    .send(Msg::Error(format!(
                        "glimpse-sunset IPC unavailable: {error}"
                    )))
                    .await;
            }
        }
    });
}

fn spawn_event_subscription(tx: glimpse_sdk::mpsc::Sender<Msg>) {
    tokio::spawn(async move {
        match ipc("sunset") {
            Ok(subscriber) => match subscriber.listen("nightlight.*").await {
                Ok(mut events) => {
                    while let Some(event) = events.next().await {
                        match event {
                            Ok(event) => {
                                let _ = tx
                                    .send(Msg::Event {
                                        name: event.name,
                                        fields: event.fields,
                                    })
                                    .await;
                            }
                            Err(error) => {
                                let _ = tx
                                    .send(Msg::Error(format!(
                                        "glimpse-sunset event read failed: {error}"
                                    )))
                                    .await;
                            }
                        }
                    }
                }
                Err(error) => {
                    let _ = tx
                        .send(Msg::Error(format!(
                            "glimpse-sunset subscription failed: {error}"
                        )))
                        .await;
                }
            },
            Err(error) => {
                let _ = tx
                    .send(Msg::Error(format!(
                        "glimpse-sunset IPC unavailable: {error}"
                    )))
                    .await;
            }
        }
    });
}

fn status_icon(state: &SunsetState) -> &'static str {
    if state.active() {
        "weather-clear-night-symbolic"
    } else {
        "weather-clear-symbolic"
    }
}

fn popover_tree(state: &SunsetState) -> TreeNode<Msg> {
    if state.sunset_socket_missing() {
        let empty = EmptyState::new("glimpse-sunset socket not found");
        let mut shell = PopoverShell::new(tree![empty]);
        shell.size = PopoverSize::Medium;
        return shell.into();
    }

    let mut hero = Hero::new("Night light", hero_subtitle(state));
    hero.icon = Some(status_icon(state).into());

    let mut switch = SwitchTile::new("night-light", "Night light");
    switch.left_icon = Some(status_icon(state).into());
    switch.active = state.active();
    switch.on_toggle = Some(MsgMapper::new(Msg::ToggleNightLight));

    let mut slider = SliderTile::new("temperature");
    slider.label = Some(format!("{} K", state.target_kelvin));
    slider.left_icon = Some("preferences-color-symbolic".into());
    slider.min = 1000.0;
    slider.max = f64::from(DAYLIGHT_KELVIN);
    slider.step = 100.0;
    slider.snap_step = Some(100.0);
    slider.value = f64::from(state.target_kelvin.clamp(1000, DAYLIGHT_KELVIN));
    slider.on_change = Some(MsgMapper::new(|value: f64| {
        Msg::SetTemperature(clamp_temperature(value.round() as u32))
    }));

    let mut schedule = ChoiceList::new(
        "schedule",
        vec![
            Choice {
                id: "off".into(),
                primary: "Off".into(),
                secondary: None,
                icon: Some("weather-clear-symbolic".into()),
            },
            Choice {
                id: "automatic".into(),
                primary: "Automatic".into(),
                secondary: None,
                icon: Some("weather-clear-night-symbolic".into()),
            },
            Choice {
                id: "schedule".into(),
                primary: "Schedule".into(),
                secondary: None,
                icon: Some("preferences-system-time-symbolic".into()),
            },
        ],
    );
    schedule.active = Some(normalize_schedule(&state.schedule).unwrap_or("off").into());
    schedule.on_change = Some(MsgMapper::new(Msg::SetSchedule));

    let mut schedule_expander = ExpanderTile::new("Schedule");
    schedule_expander.left_icon = Some("preferences-system-time-symbolic".into());
    schedule_expander.child = Some(Box::new(schedule.into()));

    let mut children = tree![
        hero,
        BoxedList::new(tree![switch, slider]),
        schedule_expander,
    ];
    if let Some(error) = &state.last_error {
        children.push(Label::new(error.clone()).into());
    }

    let mut shell = PopoverShell::new(children);
    shell.size = PopoverSize::Medium;
    shell.into()
}

fn clamp_temperature(kelvin: u32) -> u32 {
    kelvin.clamp(1000, DAYLIGHT_KELVIN)
}

fn normalize_schedule(schedule: &str) -> Option<&'static str> {
    match schedule {
        "off" => Some("off"),
        "automatic" => Some("automatic"),
        "schedule" | "manual" => Some("schedule"),
        _ => None,
    }
}

fn hero_subtitle(state: &SunsetState) -> String {
    if let Some(mins) = activation_minutes_remaining(state) {
        if mins == 0 {
            return "Activating".into();
        }
        return format!("Activates in {}m", mins);
    }
    human_phase(state)
}

fn human_phase(state: &SunsetState) -> String {
    match state.phase.as_str() {
        "night" | "transition_to_night" => "Night".into(),
        "disabled" => "Off".into(),
        _ => "Day".into(),
    }
}

fn activation_minutes_remaining(state: &SunsetState) -> Option<i64> {
    if state.phase != "day" {
        return None;
    }
    let sunset = state.next_sunset.as_deref()?;
    let now = chrono::Local::now().time();
    let sunset_time = NaiveTime::parse_from_str(sunset, "%H:%M").ok()?;
    let diff = sunset_time.signed_duration_since(now);
    let mins = diff.num_minutes();
    if (0..=60).contains(&mins) { Some(mins) } else { None }
}

#[tokio::main]
async fn main() -> glimpse_sdk::AppletResult<()> {
    run(SunsetApplet, SunsetState::default())
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::collections::HashMap;

    #[test]
    fn status_fields_update_state_snapshot() {
        let mut state = SunsetState::default();
        state.apply_status_fields(fields([
            ("phase", "night"),
            ("kelvin", "3600"),
            ("target_kelvin", "3600"),
            ("schedule", "automatic"),
            ("health", "ready"),
        ]));

        assert_eq!(state.phase, "night");
        assert_eq!(state.effective_kelvin, 3600);
        assert_eq!(state.target_kelvin, 3600);
        assert_eq!(state.schedule, "automatic");
        assert_eq!(state.health, "ready");
        assert!(state.active());
    }

    #[test]
    fn nightlight_events_update_only_reported_fields() {
        let mut state = SunsetState {
            phase: "day".into(),
            effective_kelvin: 6500,
            target_kelvin: 3900,
            schedule: "automatic".into(),
            health: "starting".into(),
            ..SunsetState::default()
        };

        state.apply_event(
            "nightlight.temperature_changed",
            fields([("kelvin", "3900"), ("phase", "transition_to_night")]),
        );
        state.apply_event("nightlight.health_changed", fields([("health", "ready")]));

        assert_eq!(state.phase, "transition_to_night");
        assert_eq!(state.effective_kelvin, 3900);
        assert_eq!(state.target_kelvin, 3900);
        assert_eq!(state.schedule, "automatic");
        assert_eq!(state.health, "ready");
    }

    #[tokio::test]
    async fn status_is_icon_only_and_uses_night_icon_when_enabled() {
        let applet = SunsetApplet;
        let state = SunsetState {
            phase: "night".into(),
            health: "ready".into(),
            schedule: "automatic".into(),
            effective_kelvin: 3600,
            target_kelvin: 3600,
            last_error: None,
        };

        let status = applet.status(&state).await.expect("status should render");

        assert_eq!(status[0].id.as_deref(), Some("sunset"));
        assert_eq!(
            status[0].icon.as_deref(),
            Some("weather-clear-night-symbolic")
        );
        assert_eq!(status[0].label, None);
        assert_eq!(
            status[0].tooltip.as_deref(),
            Some("Night light: night, ready")
        );
    }

    #[tokio::test]
    async fn status_is_icon_only_and_uses_day_icon_when_disabled() {
        let applet = SunsetApplet;

        let status = applet
            .status(&SunsetState::default())
            .await
            .expect("status should render");

        assert_eq!(status[0].icon.as_deref(), Some("weather-clear-symbolic"));
        assert_eq!(status[0].label, None);
    }

    #[test]
    fn interactive_messages_map_to_ipc_commands() {
        assert_eq!(
            ipc_commands_for_msg(&Msg::SetTemperature(3900)),
            vec![IpcCommandPlan {
                action: "set_temperature",
                params: vec![("kelvin", "3900".into())],
            }]
        );
        assert_eq!(
            ipc_commands_for_msg(&Msg::ToggleNightLight(false)),
            vec![IpcCommandPlan {
                action: "disable",
                params: Vec::new(),
            }]
        );
        assert_eq!(
            ipc_commands_for_msg(&Msg::SetSchedule("automatic".into())),
            vec![IpcCommandPlan {
                action: "set_schedule",
                params: vec![("schedule", "automatic".into())],
            }]
        );
    }

    #[test]
    fn toggle_on_enables_and_activates_night_light() {
        assert_eq!(
            ipc_commands_for_msg(&Msg::ToggleNightLight(true)),
            vec![
                IpcCommandPlan {
                    action: "enable",
                    params: Vec::new(),
                },
                IpcCommandPlan {
                    action: "activate",
                    params: Vec::new(),
                },
            ]
        );
    }

    #[test]
    fn popover_renders_empty_state_when_sunset_socket_is_missing() {
        let state = SunsetState {
            last_error: Some(
                "glimpse-sunset status failed: cannot connect to IPC socket at /run/user/1000/glimpse/sunset.sock: No such file or directory"
                    .into(),
            ),
            ..SunsetState::default()
        };

        let value = serde_json::to_value(popover_tree(&state)).expect("popover should serialize");
        let empty = find_first_type(&value, "empty_state").expect("empty state should exist");

        assert_eq!(empty["data"]["title"], "glimpse-sunset socket not found");
        assert!(empty["data"].get("subtitle").is_none());
    }

    #[test]
    fn popover_hero_subtitle_displays_status() {
        let state = SunsetState {
            phase: "night".into(),
            health: "ready".into(),
            schedule: "automatic".into(),
            target_kelvin: 3900,
            ..SunsetState::default()
        };

        let value = serde_json::to_value(popover_tree(&state)).expect("popover should serialize");
        let hero = find_first_type(&value, "hero").expect("hero should exist");

        assert_eq!(hero["data"]["subtitle"], "Night");
    }

    #[test]
    fn popover_contains_only_requested_controls() {
        let state = SunsetState {
            schedule: "automatic".into(),
            target_kelvin: 3900,
            ..SunsetState::default()
        };

        let value = serde_json::to_value(popover_tree(&state)).expect("popover should serialize");

        assert!(find_text(&value, "Night light"));
        assert!(find_text(&value, "3900 K"));
        assert!(find_text(&value, "Schedule"));
        assert!(!find_text(&value, "Refresh"));
        assert!(!find_text(&value, "Reset to config"));
    }

    #[test]
    fn popover_renders_schedule_choice_list() {
        let state = SunsetState {
            schedule: "schedule".into(),
            ..SunsetState::default()
        };

        let value = serde_json::to_value(popover_tree(&state)).expect("popover should serialize");
        let list = find_first_type(&value, "choice_list").expect("choice list should exist");

        assert_eq!(list["data"]["id"], "schedule");
        assert_eq!(list["data"]["active"], "schedule");
        assert_eq!(list["data"]["choices"][0]["id"], "off");
        assert_eq!(list["data"]["choices"][0]["primary"], "Off");
        assert_eq!(list["data"]["choices"][1]["id"], "automatic");
        assert_eq!(list["data"]["choices"][1]["primary"], "Automatic");
        assert_eq!(list["data"]["choices"][2]["id"], "schedule");
        assert_eq!(list["data"]["choices"][2]["primary"], "Schedule");
    }

    #[test]
    fn schedule_choice_list_is_child_of_expander_tile() {
        let state = SunsetState {
            schedule: "automatic".into(),
            ..SunsetState::default()
        };

        let value = serde_json::to_value(popover_tree(&state)).expect("popover should serialize");
        let expander = find_first_type(&value, "expander_tile").expect("expander should exist");

        assert_eq!(expander["data"]["primary"], "Schedule");
        assert!(expander["data"].get("secondary").is_none());
        assert_eq!(expander["data"]["child"]["type"], "choice_list");
        assert_eq!(expander["data"]["child"]["data"]["id"], "schedule");
    }

    #[tokio::test]
    async fn update_applies_schedule_choice_locally() {
        let mut applet = SunsetApplet;
        let mut state = SunsetState {
            phase: "night".into(),
            effective_kelvin: 3900,
            ..SunsetState::default()
        };

        applet
            .update(&mut state, Msg::SetSchedule("off".into()))
            .await
            .expect("schedule update should apply");

        assert_eq!(state.schedule, "off");
        assert_eq!(state.phase, "disabled");
        assert_eq!(state.effective_kelvin, DAYLIGHT_KELVIN);
    }

    #[test]
    fn switch_is_off_when_schedule_is_enabled_but_effect_is_daylight() {
        let state = SunsetState {
            schedule: "automatic".into(),
            phase: "day".into(),
            effective_kelvin: DAYLIGHT_KELVIN,
            target_kelvin: 3900,
            ..SunsetState::default()
        };

        let value = serde_json::to_value(popover_tree(&state)).expect("popover should serialize");
        let switch = find_first_type(&value, "switch_tile").expect("switch should exist");

        assert_eq!(switch["data"]["active"], false);
        assert!(switch["data"].get("secondary").is_none());
    }

    fn fields<const N: usize>(pairs: [(&str, &str); N]) -> HashMap<String, String> {
        pairs
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect()
    }

    fn find_first_type<'a>(value: &'a Value, kind: &str) -> Option<&'a Value> {
        if value.get("type").and_then(Value::as_str) == Some(kind) {
            return Some(value);
        }
        match value {
            Value::Array(items) => items.iter().find_map(|item| find_first_type(item, kind)),
            Value::Object(object) => object.values().find_map(|item| find_first_type(item, kind)),
            _ => None,
        }
    }

    fn find_text(value: &Value, needle: &str) -> bool {
        match value {
            Value::String(text) => text == needle,
            Value::Array(items) => items.iter().any(|item| find_text(item, needle)),
            Value::Object(object) => object.values().any(|item| find_text(item, needle)),
            _ => false,
        }
    }
}
