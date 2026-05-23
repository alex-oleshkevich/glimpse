use async_trait::async_trait;
use glimpse_sdk::{
    Applet, AppletResult, BoxedList, Column, EmptyState, Hero, MsgMapper, PopoverShell,
    PopoverSize, SegmentedTile, StatusItem, Text, Tile, TreeNode, run, tree,
};
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::sync::mpsc;
use tokio::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Device {
    id: String,
    name: String,
    charge: Option<u8>,
}

#[derive(Debug, Default, Clone)]
struct State {
    devices: Vec<Device>,
    expanded: HashSet<String>,
    known_devices: HashSet<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Msg {
    Refresh,
    ToggleDevice(String, bool),
    RunAction(String, DeviceAction),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceAction {
    Ping,
    Ring,
    Browse,
    Clipboard,
}

struct KdeConnectApplet;

#[async_trait]
impl Applet for KdeConnectApplet {
    type State = State;
    type Msg = Msg;

    async fn status(&self, state: &State) -> AppletResult<Vec<StatusItem>> {
        if state.devices.is_empty() {
            return Ok(Vec::new());
        }

        Ok(vec![
            StatusItem::new("kdeconnect")
                .icon("phone-symbolic")
                .tooltip(connected_summary(state.devices.len())),
        ])
    }

    async fn on_start(&mut self, state: &mut State, tx: mpsc::Sender<Msg>) -> AppletResult<()> {
        refresh_state(state).await;

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(15)).await;
                if tx.send(Msg::Refresh).await.is_err() {
                    break;
                }
            }
        });

        Ok(())
    }

    async fn update(&mut self, state: &mut State, msg: Msg) -> AppletResult<()> {
        match msg {
            Msg::Refresh => {
                refresh_state(state).await;
            }
            Msg::ToggleDevice(id, expanded) => {
                if expanded {
                    state.expanded.insert(id);
                } else {
                    state.expanded.remove(&id);
                }
            }
            Msg::RunAction(device_id, action) => {
                if let Err(err) = run_device_action(&device_id, action).await {
                    state.error = Some(err.to_string());
                } else {
                    state.error = None;
                }
            }
        }
        Ok(())
    }

    async fn popover(&self, state: &State) -> AppletResult<Option<TreeNode<Msg>>> {
        Ok(Some(popover_tree(state)))
    }
}

fn parse_devices(ids_stdout: &str, names_stdout: &str) -> Vec<Device> {
    ids_stdout
        .lines()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .zip(
            names_stdout
                .lines()
                .map(str::trim)
                .filter(|name| !name.is_empty()),
        )
        .map(|(id, name)| Device {
            id: id.to_owned(),
            name: name.to_owned(),
            charge: None,
        })
        .collect()
}

fn reconcile_expanded(state: &mut State) {
    let visible: HashSet<String> = state
        .devices
        .iter()
        .map(|device| device.id.clone())
        .collect();
    state.expanded.retain(|id| visible.contains(id));
    state.known_devices = visible;
}

fn popover_tree(state: &State) -> TreeNode<Msg> {
    if state.devices.is_empty() {
        let mut empty = EmptyState::new("No connected devices");
        empty.subtitle = state
            .error
            .clone()
            .or_else(|| Some("Pair or connect a device with KDE Connect.".into()));
        return empty.into();
    }

    let device_tiles = state
        .devices
        .iter()
        .map(|device| device_tile(device, state.expanded.contains(&device.id)).into())
        .collect();
    let mut body: Vec<TreeNode<Msg>> = vec![BoxedList::new(device_tiles).into()];
    if let Some(error) = &state.error {
        body.push(Text::new(error.clone()).into());
    }

    let mut shell = PopoverShell::new(tree![
        {
            let mut hero = Hero::new("KDE Connect", connected_summary(state.devices.len()));
            hero.icon = Some("phone-symbolic".into());
            hero
        },
        Column::new(body),
    ]);
    shell.size = PopoverSize::Medium;
    shell.into()
}

fn device_tile(device: &Device, expanded: bool) -> SegmentedTile<Msg> {
    let action_children = vec![
        action_tile(
            &device.id,
            "ping",
            "Ping device",
            "network-transmit-receive-symbolic",
            DeviceAction::Ping,
        )
        .into(),
        action_tile(
            &device.id,
            "ring",
            "Find device",
            "audio-volume-high-symbolic",
            DeviceAction::Ring,
        )
        .into(),
        action_tile(
            &device.id,
            "browse",
            "Browse files",
            "folder-symbolic",
            DeviceAction::Browse,
        )
        .into(),
        action_tile(
            &device.id,
            "clipboard",
            "Share clipboard",
            "edit-paste-symbolic",
            DeviceAction::Clipboard,
        )
        .into(),
    ];

    let id = device.id.clone();
    let mut tile = SegmentedTile::new(device.name.clone());
    tile.id = Some(format!("device-{}", device.id));
    tile.left_icon = Some("phone-symbolic".into());
    if let Some(charge) = device.charge {
        tile.right = Some(Box::new(battery_status_text(charge).into()));
    }
    tile.child = Some(Box::new(BoxedList::new(action_children).into()));
    tile.expanded = expanded;
    tile.on_toggle = Some(MsgMapper::new(move |expanded| {
        Msg::ToggleDevice(id.clone(), expanded)
    }));
    tile
}

fn action_tile(
    device_id: &str,
    id: &str,
    label: &str,
    icon: &str,
    action: DeviceAction,
) -> Tile<Msg> {
    let device_id = device_id.to_owned();
    let mut tile = Tile::new(label);
    tile.id = Some(format!("{id}-{device_id}"));
    tile.left_icon = Some(icon.into());
    tile.activatable = true;
    tile.on_click = Some(MsgMapper::new(move |()| {
        Msg::RunAction(device_id.clone(), action)
    }));
    tile
}

fn connected_summary(count: usize) -> String {
    match count {
        1 => "1 device connected".into(),
        n => format!("{n} devices connected"),
    }
}

fn battery_status_text(charge: u8) -> Text {
    Text::new(format!("{charge}%"))
        .css_class("dim-label")
        .css_class("caption")
        .css_class("numeric")
}

async fn refresh_state(state: &mut State) {
    match connected_devices().await {
        Ok(devices) => {
            state.devices = devices;
            state.error = None;
            reconcile_expanded(state);
        }
        Err(err) => {
            state.devices.clear();
            state.expanded.clear();
            state.error = Some(err.to_string());
        }
    }
}

async fn connected_devices() -> AppletResult<Vec<Device>> {
    let ids = kdeconnect_output(&["--list-available", "--id-only"]).await?;
    let names = kdeconnect_output(&["--list-available", "--name-only"]).await?;
    let mut devices = parse_devices(&ids, &names);
    for device in &mut devices {
        device.charge = device_charge(&device.id).await.unwrap_or(None);
    }
    Ok(devices)
}

async fn run_device_action(device_id: &str, action: DeviceAction) -> AppletResult<()> {
    match action {
        DeviceAction::Ping => {
            kdeconnect_output(&["--device", device_id, "--ping"]).await?;
        }
        DeviceAction::Ring => {
            kdeconnect_output(&["--device", device_id, "--ring"]).await?;
        }
        DeviceAction::Clipboard => {
            kdeconnect_output(&["--device", device_id, "--send-clipboard"]).await?;
        }
        DeviceAction::Browse => {
            kdeconnect_output(&["--device", device_id, "--mount"]).await?;
            let mount = kdeconnect_output(&["--device", device_id, "--get-mount-point"]).await?;
            let mount = mount.trim();
            if mount.is_empty() {
                return Err("KDE Connect did not return a mount point".into());
            }
            open_path(PathBuf::from(mount)).await?;
        }
    }

    Ok(())
}

async fn kdeconnect_output(args: &[&str]) -> AppletResult<String> {
    let output = command_output("kdeconnect-cli", args, Duration::from_secs(5)).await?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn device_charge(device_id: &str) -> AppletResult<Option<u8>> {
    let path = format!(
        "/modules/kdeconnect/devices/{}/battery",
        device_id.replace('-', "_")
    );
    let output = command_output(
        "busctl",
        &[
            "--user",
            "get-property",
            "org.kde.kdeconnect",
            &path,
            "org.kde.kdeconnect.device.battery",
            "charge",
        ],
        Duration::from_secs(2),
    )
    .await?;

    Ok(parse_charge(&String::from_utf8_lossy(&output.stdout)))
}

fn parse_charge(output: &str) -> Option<u8> {
    let value = output.split_whitespace().nth(1)?.parse::<u8>().ok()?;
    (value <= 100).then_some(value)
}

async fn command_output(
    program: &str,
    args: &[&str],
    limit: Duration,
) -> AppletResult<std::process::Output> {
    let mut command = tokio::process::Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let output = match tokio::time::timeout(limit, command.output()).await {
        Ok(output) => output?,
        Err(_) => return Err(format!("{program} timed out after {limit:?}").into()),
    };

    if output.status.success() {
        return Ok(output);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.is_empty() {
        Err(format!("kdeconnect-cli exited with {}", output.status).into())
    } else {
        Err(stderr.into())
    }
}

async fn open_path(path: PathBuf) -> AppletResult<()> {
    let path = path.to_string_lossy();
    command_output("xdg-open", &[path.as_ref()], Duration::from_secs(5)).await?;
    Ok(())
}

#[tokio::main]
async fn main() -> AppletResult<()> {
    run(KdeConnectApplet, State::default()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_id_and_name_lists_without_splitting_device_names() {
        let devices = parse_devices("id-1\nid-2\n", "Pixel 8 Pro\nSteam Deck\n");

        assert_eq!(
            devices,
            vec![
                Device {
                    id: "id-1".into(),
                    name: "Pixel 8 Pro".into(),
                    charge: None,
                },
                Device {
                    id: "id-2".into(),
                    name: "Steam Deck".into(),
                    charge: None,
                },
            ]
        );
    }

    #[test]
    fn reconcile_expanded_prunes_missing_devices_without_expanding_by_default() {
        let mut state = State {
            devices: vec![
                Device {
                    id: "phone".into(),
                    name: "Phone".into(),
                    charge: None,
                },
                Device {
                    id: "deck".into(),
                    name: "Steam Deck".into(),
                    charge: None,
                },
            ],
            expanded: HashSet::from(["missing".into()]),
            ..State::default()
        };

        reconcile_expanded(&mut state);

        assert!(state.expanded.is_empty());
        assert_eq!(
            state.known_devices,
            HashSet::from(["phone".into(), "deck".into()])
        );
    }

    #[test]
    fn reconcile_expanded_preserves_user_collapsed_device_on_refresh() {
        let mut state = State {
            devices: vec![Device {
                id: "phone".into(),
                name: "Phone".into(),
                charge: None,
            }],
            expanded: HashSet::from(["phone".into()]),
            known_devices: HashSet::from(["phone".into()]),
            ..State::default()
        };

        state.expanded.clear();
        reconcile_expanded(&mut state);

        assert!(state.expanded.is_empty());
    }

    #[tokio::test]
    async fn command_output_times_out_stuck_processes() {
        let err = command_output("sh", &["-c", "sleep 2"], Duration::from_millis(10))
            .await
            .expect_err("sleeping command should time out");

        assert!(err.to_string().contains("timed out"));
    }

    #[test]
    fn parses_busctl_charge_property() {
        assert_eq!(parse_charge("i 60\n"), Some(60));
        assert_eq!(parse_charge("i 100\n"), Some(100));
        assert_eq!(parse_charge("i 101\n"), None);
        assert_eq!(parse_charge(""), None);
    }

    #[tokio::test]
    async fn status_is_only_visible_when_devices_are_connected() {
        let empty = KdeConnectApplet.status(&State::default()).await.unwrap();
        assert!(empty.is_empty());

        let connected = KdeConnectApplet
            .status(&State {
                devices: vec![Device {
                    id: "phone".into(),
                    name: "Phone".into(),
                    charge: None,
                }],
                ..State::default()
            })
            .await
            .unwrap();

        assert_eq!(
            connected,
            vec![
                StatusItem::new("kdeconnect")
                    .icon("phone-symbolic")
                    .tooltip("1 device connected")
            ]
        );
    }

    #[test]
    fn popover_uses_segmented_device_tiles_with_action_children() {
        let state = State {
            devices: vec![Device {
                id: "phone".into(),
                name: "Pixel 8 Pro".into(),
                charge: Some(87),
            }],
            expanded: HashSet::from(["phone".into()]),
            ..State::default()
        };

        let tree = serde_json::to_value(popover_tree(&state)).expect("serialize popover");
        let tile = &tree["data"]["children"][1]["data"]["children"][0]["data"]["children"][0];

        assert_eq!(tile["type"], "segmented_tile");
        assert_eq!(tile["data"]["id"], "device-phone");
        assert_eq!(tile["data"]["primary"], "Pixel 8 Pro");
        assert!(tile["data"]["secondary"].is_null());
        assert_eq!(tile["data"]["left_icon"], "phone-symbolic");
        assert_eq!(tile["data"]["right"]["type"], "text");
        assert_eq!(tile["data"]["right"]["data"]["text"], "87%");
        assert_eq!(
            tile["data"]["right"]["data"]["css_classes"],
            json!(["dim-label", "caption", "numeric"])
        );
        assert_eq!(tile["data"]["expanded"], true);
        assert_eq!(
            tile["data"]["child"]["data"]["children"],
            json!([
                {
                    "type": "tile",
                    "data": {
                        "id": "ping-phone",
                        "primary": "Ping device",
                        "left_icon": "network-transmit-receive-symbolic",
                        "activatable": true
                    }
                },
                {
                    "type": "tile",
                    "data": {
                        "id": "ring-phone",
                        "primary": "Find device",
                        "left_icon": "audio-volume-high-symbolic",
                        "activatable": true
                    }
                },
                {
                    "type": "tile",
                    "data": {
                        "id": "browse-phone",
                        "primary": "Browse files",
                        "left_icon": "folder-symbolic",
                        "activatable": true
                    }
                },
                {
                    "type": "tile",
                    "data": {
                        "id": "clipboard-phone",
                        "primary": "Share clipboard",
                        "left_icon": "edit-paste-symbolic",
                        "activatable": true
                    }
                }
            ])
        );
    }
}
