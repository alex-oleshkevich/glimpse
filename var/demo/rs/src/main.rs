use async_trait::async_trait;
use glimpse_sdk::{
    ActionItem, Applet, AppletResult, Badge, Button, ButtonVariant, CallbackEvent, Card,
    Checkbox, Column, ContentFit, Copyable, EmptyState, Expander, Grid, GridChild, Hero, Icon,
    Label, LevelBar, LevelBarMode, LinkButton, ListBox, MenuButton, Meter, Orientation,
    Overlay, PagerItem, PagerStrip, Picture, Progress, PropertyList, Row, Scroll, Section, Select,
    Separator, Slider, Spinner, StatusDot, StatusItem, Switch, ToggleButton,
    TreeExpander, TreeNode, Variant, run, tree,
};

const PROFILES: [&str; 3] = ["Balanced", "Focus", "Presentation"];
const DEMO_PICTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../assets/workstation-picture.svg"
);

#[derive(Debug, Clone)]
struct DemoState {
    vpn: bool,
    quiet: bool,
    backup: bool,
    brightness: f64,
    cpu: f64,
    profile: usize,
    page: u8,
    filter: String,
    syncs: u32,
    popover_open: bool,
    last_event: String,
}

impl Default for DemoState {
    fn default() -> Self {
        Self {
            vpn: true,
            quiet: false,
            backup: true,
            brightness: 0.68,
            cpu: 0.42,
            profile: 0,
            page: 1,
            filter: String::new(),
            syncs: 3,
            popover_open: false,
            last_event: "ready".into(),
        }
    }
}

struct WorkstationApplet;

#[async_trait]
impl Applet for WorkstationApplet {
    type State = DemoState;

    async fn status(&self, state: &Self::State) -> AppletResult<Vec<StatusItem>> {
        let icon = if state.vpn {
            "network-vpn-symbolic"
        } else {
            "network-offline-symbolic"
        };
        Ok(vec![
            StatusItem::new("workstation")
                .icon(icon)
                .label(PROFILES[state.profile])
                .tooltip(state.last_event.clone()),
        ])
    }

    async fn popover(&self, state: &Self::State) -> AppletResult<Option<TreeNode>> {
        let mut strip = PagerStrip::new(vec![
            PagerItem::number("1")
                .active(state.page == 1)
                .occupied(true),
            PagerItem::number("2")
                .active(state.page == 2)
                .occupied(true),
            PagerItem::number("3")
                .active(state.page == 3)
                .urgent(state.cpu > 0.8),
        ]);
        strip.id = Some("workspace-strip".into());
        strip.common.tooltip = Some("Scroll to switch pages".into());

        let mut vpn_dot = StatusDot::new();
        vpn_dot.variant = Some(if state.vpn {
            Variant::Success
        } else {
            Variant::Warning
        });

        let mut brightness = Slider::new("brightness");
        brightness.min = 0.0;
        brightness.max = 1.0;
        brightness.step = 0.05;
        brightness.value = state.brightness;
        brightness.draw_value = true;

        let mut cpu_meter = Meter::new("CPU pressure", state.cpu, 1.0);
        cpu_meter.id = Some("cpu-meter".into());
        cpu_meter.icon = Some("utilities-system-monitor-symbolic".into());
        cpu_meter.step = 0.01;
        cpu_meter.text = Some(format!("{}%", (state.cpu * 100.0).round()));
        cpu_meter.interactive = true;

        let mut profile = Select::new(
            "profile",
            PROFILES
                .iter()
                .enumerate()
                .map(|(index, label)| (index.to_string(), (*label).into()))
                .collect(),
        );
        profile.selected = Some(state.profile as u32);

        let mut info_icon = Icon::new("dialog-information-symbolic");
        info_icon.pixel_size = Some(20);

        Ok(Some(
            Column::new(tree![
                Hero::new(
                    "Workstation",
                    if state.popover_open {
                        "Controls are live"
                    } else {
                        "Popover is closing"
                    }
                )
                .icon("computer-symbolic"),
                strip,
                Grid::new(vec![
                    GridChild::new(
                        0,
                        0,
                        metric_card(
                            "CPU",
                            format!("{}%", (state.cpu * 100.0).round()),
                            "view-statistics-symbolic"
                        )
                    ),
                    GridChild::new(
                        0,
                        1,
                        metric_card(
                            "Brightness",
                            format!("{}%", (state.brightness * 100.0).round()),
                            "display-brightness-symbolic"
                        )
                    ),
                    GridChild::new(
                        1,
                        0,
                        metric_card("Syncs", state.syncs.to_string(), "view-refresh-symbolic")
                    ),
                    GridChild::new(1, 1, vpn_dot.into()),
                ]),
                Section::new(
                    "Controls",
                    Some(Column::new(tree![
                        Row::new(tree![
                            Button::new("sync-now")
                                .label("Sync")
                                .icon("view-refresh-symbolic")
                                .variant(ButtonVariant::Primary),
                            Button::new("quiet")
                                .label(if state.quiet { "Quiet" } else { "Focus" })
                                .icon("notifications-disabled-symbolic")
                                .variant(ButtonVariant::Secondary),
                            Button::new("danger")
                                .label("Reset")
                                .icon("edit-delete-symbolic")
                                .enabled(false)
                                .variant(ButtonVariant::Danger),
                        ])
                        .spacing(8),
                        switch("vpn-toggle", "VPN tunnel", state.vpn),
                        toggle_button("focus-toggle", "Focus mode", state.quiet),
                        checkbox("backup-toggle", "Nightly backups", state.backup),
                        brightness,
                        cpu_meter,
                        LevelBar::new(state.cpu)
                            .min(0.0)
                            .max(1.0)
                            .mode(LevelBarMode::Continuous),
                        MenuButton::new(
                            Column::new(tree![Label::new("Quick actions"), Badge::new("rendered")])
                                .spacing(4),
                        )
                        .label("Menu")
                        .icon("open-menu-symbolic"),
                        profile,
                    ])
                    .into()),
                )
                .subtitle("Daily workstation settings"),
                Section::new(
                    "Queue",
                    Some(Column::new(tree![
                        ActionItem::new("open-terminal", "Terminal session")
                            .icon("utilities-terminal-symbolic")
                            .sublabel(if state.vpn { "Secure session" } else { "Offline" })
                            .right(Button::new("open-terminal-indicator")
                                .icon("utilities-terminal-symbolic")
                                .variant(ButtonVariant::Flat)),
                        ListBox::new(tree![
                            Row::new(tree![Label::new("Build cache"), Badge::new("running")])
                                .spacing(8),
                            Row::new(tree![Label::new("Backup job"), Badge::new("scheduled")])
                                .spacing(8),
                        ]),
                        TreeExpander::new(Label::new("Nested queue row"))
                            .hide_expander(true)
                            .indent_for_depth(true)
                            .indent_for_icon(true),
                        background_jobs(state.backup),
                    ])
                    .into()),
                ),
                Card::new(Some(Column::new(tree![
                    Row::new(tree![
                        Spinner::new().spinning(state.syncs % 2 == 0),
                        info_icon,
                        wrapped_label("Filter input is handled through input callbacks."),
                    ])
                    .spacing(8),
                    Copyable::new("Host", "devbox.local"),
                    LinkButton::new("https://example.com/docs").label("Docs"),
                    Expander::new("Session details").expanded(state.popover_open).child(
                        Column::new(tree![
                            Label::new(format!("Profile: {}", PROFILES[state.profile])),
                            Label::new(format!("Last event: {}", state.last_event)),
                        ])
                        .spacing(4),
                    ),
                    Overlay::new(Picture::new(DEMO_PICTURE_PATH).content_fit(ContentFit::Cover))
                        .overlay(Badge::new("Live")),
                    PropertyList::new([
                        ("Profile", PROFILES[state.profile].to_string()),
                        ("Last event", state.last_event.clone()),
                        (
                            "Filter",
                            if state.filter.is_empty() {
                                "none".into()
                            } else {
                                state.filter.clone()
                            },
                        ),
                    ])
                    .title("Session"),
                ])
                .into())),
                activity_area(state),
                horizontal_separator(),
                Row::new(tree![
                    Badge::new("SDK"),
                    muted_label("All components covered"),
                ])
                .spacing(6),
            ])
            .spacing(10)
            .into(),
        ))
    }

    async fn on_callback(
        &mut self,
        state: &mut Self::State,
        event: CallbackEvent,
    ) -> AppletResult<()> {
        match event {
            CallbackEvent::Click(click) => match click.id.as_str() {
                "sync-now" => {
                    state.syncs += 1;
                    state.last_event = "manual sync requested".into();
                }
                "quiet" => {
                    state.quiet = !state.quiet;
                    state.last_event = "quiet mode toggled".into();
                }
                "danger" => state.last_event = "destructive action blocked in demo".into(),
                "open-terminal" => state.last_event = "terminal shortcut selected".into(),
                _ => {}
            },
            CallbackEvent::Toggle(toggle) => match toggle.id.as_str() {
                "vpn-toggle" => {
                    state.vpn = toggle.value;
                    state.last_event = format!("vpn: {}", toggle.value);
                }
                "backup-toggle" => {
                    state.backup = toggle.value;
                    state.last_event = format!("backup: {}", toggle.value);
                }
                "focus-toggle" => {
                    state.quiet = toggle.value;
                    state.last_event = format!("focus: {}", toggle.value);
                }
                _ => {}
            },
            CallbackEvent::Change(change) => match change.id.as_str() {
                "brightness" => {
                    state.brightness = change
                        .value
                        .and_then(|value| value.as_f64())
                        .unwrap_or(state.brightness);
                    state.last_event = "brightness changed".into();
                }
                "cpu-meter" => {
                    state.cpu = change
                        .value
                        .and_then(|value| value.as_f64())
                        .unwrap_or(state.cpu);
                    state.last_event = "cpu changed".into();
                }
                "profile" => {
                    state.profile = selected_index(change.value).min(PROFILES.len() - 1);
                    state.last_event = "profile changed".into();
                }
                _ => {}
            },
            CallbackEvent::Input(input) => {
                if input.id == "filter" {
                    state.filter = input.text;
                    state.last_event = format!("filter: {}", state.filter);
                }
            }
            CallbackEvent::Scroll(scroll) => {
                if scroll.id == "workspace-strip" {
                    let delta = if scroll.delta_y.unwrap_or_default() > 0.0 {
                        1
                    } else {
                        2
                    };
                    state.page = ((state.page + delta - 1) % 3) + 1;
                    state.last_event = format!("workspace {}", state.page);
                }
            }
            CallbackEvent::Popover(popover) => {
                state.popover_open = popover.open;
                state.last_event = if popover.open {
                    "popover open"
                } else {
                    "popover close"
                }
                .into();
            }
        }
        Ok(())
    }
}

fn metric_card(label: &str, value: String, icon_name: &str) -> TreeNode {
    let mut icon = Icon::new(icon_name);
    icon.pixel_size = Some(18);
    let ratio = value.trim_end_matches('%').parse::<f64>().unwrap_or(50.0) / 100.0;
    Card::new(Some(Column::new(tree![
        Row::new(tree![icon, Label::new(label)]).spacing(6),
        Progress::new(ratio).max(1.0).text(value).show_text(true),
    ])
    .into()))
    .into()
}

fn switch(id: &str, label: &str, active: bool) -> Switch {
    let mut widget = Switch::new(id);
    widget.label = Some(label.into());
    widget.active = active;
    widget
}

fn checkbox(id: &str, label: &str, active: bool) -> Checkbox {
    let mut widget = Checkbox::new(id);
    widget.label = Some(label.into());
    widget.active = active;
    widget
}

fn toggle_button(id: &str, label: &str, active: bool) -> ToggleButton {
    let mut widget = ToggleButton::new(id);
    widget.label = Some(label.into());
    widget.active = active;
    widget
}

fn background_jobs(backup: bool) -> Section {
    Section::new(
        "Background jobs",
        Some(Column::new(tree![
            Row::new(tree![Label::new("Index packages"), wrapped_label("Index packages")])
                .spacing(8),
            Row::new(tree![
                Label::new("Backup window"),
                muted_label(if backup { "02:00" } else { "Paused" }),
            ])
            .spacing(8),
        ])
        .into()),
    )
    .subtitle("Build, backup, and indexing")
}

fn wrapped_label(text: &str) -> Label {
    let mut label = Label::new(text);
    label.wrap = true;
    label
}

fn muted_label(text: impl Into<String>) -> Label {
    let mut label = Label::new(text);
    label.variant = Some(Variant::Muted);
    label
}

fn horizontal_separator() -> Separator {
    let mut separator = Separator::new();
    separator.orientation = Some(Orientation::Horizontal);
    separator
}

fn activity_area(state: &DemoState) -> TreeNode {
    if state.filter.is_empty() {
        EmptyState::new("No filtered activity")
            .subtitle("Type in the shell-provided input callback to populate this area.")
            .into()
    } else {
        Scroll::new(
            Column::new(tree![
                muted_recent_label(),
                Label::new("VPN checked"),
                Label::new("Backups scheduled"),
            ])
            .spacing(4)
            .into(),
        )
        .into()
    }
}

fn muted_recent_label() -> Label {
    muted_label("Recent activity")
}

fn selected_index(value: Option<serde_json::Value>) -> usize {
    match value {
        Some(serde_json::Value::Number(number)) => number.as_u64().unwrap_or_default() as usize,
        Some(serde_json::Value::Object(map)) => map
            .get("index")
            .and_then(|value| value.as_u64())
            .unwrap_or_default() as usize,
        _ => 0,
    }
}

#[tokio::main]
async fn main() -> AppletResult<()> {
    run(WorkstationApplet, DemoState::default()).await
}
