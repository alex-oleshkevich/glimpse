use async_trait::async_trait;
use futures_util::StreamExt;
use glimpse_sdk::{
    Applet, AppletResult, BoxedList, Column, EmptyState, Hero, Label, MsgMapper, PopoverShell,
    PopoverSize, SegmentedTile, StatusItem, Tile, TreeNode, run, tree,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::sync::mpsc;
use tokio::time::Duration;
use zbus::{MatchRule, MessageStream, message::Type};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Device {
    id: String,
    name: String,
    charge: Option<u8>,
}

impl Device {
    fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            charge: None,
        }
    }
}

#[derive(Debug, Default, Clone)]
struct State {
    devices: Vec<Device>,
    expanded: HashSet<String>,
    known_devices: HashSet<String>,
    mirrored_notifications: HashMap<String, u32>,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Msg {
    Refresh,
    ToggleDevice(String, bool),
    RunAction(String, DeviceAction),
    MirrorNotification(KdeNotification),
    RemoveNotification(String),
    RemoveDeviceNotifications(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceAction {
    Ping,
    Ring,
    Browse,
    Clipboard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct KdeNotification {
    key: String,
    device_name: String,
    app_name: String,
    summary: String,
    body: String,
    icon: Option<String>,
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

        let refresh_tx = tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(15)).await;
                if refresh_tx.send(Msg::Refresh).await.is_err() {
                    break;
                }
            }
        });

        tokio::spawn(async move {
            eprintln!("kdeconnect notification bridge: starting");
            if let Err(err) = watch_phone_notifications(tx).await {
                eprintln!("kdeconnect notification bridge failed: {err}");
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
            Msg::MirrorNotification(notification) => {
                let replaces_id = state
                    .mirrored_notifications
                    .get(&notification.key)
                    .copied()
                    .unwrap_or_default();
                match mirror_desktop_notification(&notification, replaces_id).await {
                    Ok(local_id) => {
                        state
                            .mirrored_notifications
                            .insert(notification.key, local_id);
                        state.error = None;
                    }
                    Err(err) => {
                        state.error = Some(err.to_string());
                    }
                }
            }
            Msg::RemoveNotification(key) => {
                if let Some(local_id) = state.mirrored_notifications.remove(&key)
                    && let Err(err) = close_desktop_notification(local_id).await
                {
                    state.error = Some(err.to_string());
                }
            }
            Msg::RemoveDeviceNotifications(device_id) => {
                let prefix = format!("{device_id}:");
                let keys: Vec<String> = state
                    .mirrored_notifications
                    .keys()
                    .filter(|key| key.starts_with(&prefix))
                    .cloned()
                    .collect();
                for key in keys {
                    if let Some(local_id) = state.mirrored_notifications.remove(&key)
                        && let Err(err) = close_desktop_notification(local_id).await
                    {
                        state.error = Some(err.to_string());
                    }
                }
            }
        }
        Ok(())
    }

    async fn popover(&self, state: &State) -> AppletResult<Option<TreeNode<Msg>>> {
        Ok(Some(popover_tree(state)))
    }
}

fn parse_device_paths(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter_map(|line| line.rsplit_once(' ').map(|(_, path)| path).or(Some(line)))
        .map(str::trim)
        .filter(|path| path.starts_with("/modules/kdeconnect/devices/"))
        .filter(|path| {
            !path
                .trim_start_matches("/modules/kdeconnect/devices/")
                .contains('/')
        })
        .map(|path| path.trim_start_matches("/modules/kdeconnect/devices/"))
        .map(|segment| segment.replace('_', "-"))
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
        body.push(Label::new(error.clone()).into());
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
            "Send clipboard to phone",
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

fn battery_status_text(charge: u8) -> Label {
    Label::new(format!("{charge}%"))
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
    let tree = command_text(
        "busctl",
        &["--user", "tree", "org.kde.kdeconnect"],
        Duration::from_secs(2),
    )
    .await?;
    let mut devices = Vec::new();
    for id in parse_device_paths(&tree) {
        if !device_bool_property(&id, "isPaired")
            .await?
            .unwrap_or(false)
        {
            continue;
        }
        if !device_bool_property(&id, "isReachable")
            .await?
            .unwrap_or(false)
        {
            continue;
        }
        let Some(name) = device_string_property(&id, "name").await? else {
            continue;
        };
        let mut device = Device::new(id, name);
        device.charge = device_charge(&device.id).await.unwrap_or(None);
        devices.push(device);
    }
    Ok(devices)
}

async fn run_device_action(device_id: &str, action: DeviceAction) -> AppletResult<()> {
    match action {
        DeviceAction::Ping => {
            dbus_call(
                &format!("{}/ping", device_path(device_id)),
                "org.kde.kdeconnect.device.ping",
                "sendPing",
                &[],
            )
            .await?
        }
        DeviceAction::Ring => {
            dbus_call(
                &format!("{}/findmyphone", device_path(device_id)),
                "org.kde.kdeconnect.device.findmyphone",
                "ring",
                &[],
            )
            .await?
        }
        DeviceAction::Clipboard => {
            dbus_call(
                &format!("{}/clipboard", device_path(device_id)),
                "org.kde.kdeconnect.device.clipboard",
                "sendClipboard",
                &[],
            )
            .await?
        }
        DeviceAction::Browse => {
            let sftp_path = format!("{}/sftp", device_path(device_id));
            let mounted = dbus_call_text(
                &sftp_path,
                "org.kde.kdeconnect.device.sftp",
                "mountAndWait",
                &[],
            )
            .await?;
            if parse_busctl_bool(&mounted) != Some(true) {
                return Err("KDE Connect could not mount the device filesystem".into());
            }
            let mount = dbus_call_text(
                &sftp_path,
                "org.kde.kdeconnect.device.sftp",
                "mountPoint",
                &[],
            )
            .await?;
            let mount = parse_busctl_string(&mount).unwrap_or_default();
            if mount.trim().is_empty() {
                return Err("KDE Connect did not return a mount point".into());
            }
            open_path(PathBuf::from(mount)).await?;
        }
    }

    Ok(())
}

async fn command_text(program: &str, args: &[&str], limit: Duration) -> AppletResult<String> {
    let output = command_output(program, args, limit).await?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn dbus_call(path: &str, interface: &str, method: &str, extra: &[&str]) -> AppletResult<()> {
    dbus_call_text(path, interface, method, extra).await?;
    Ok(())
}

async fn dbus_call_text(
    path: &str,
    interface: &str,
    method: &str,
    extra: &[&str],
) -> AppletResult<String> {
    let mut args = vec![
        "--user",
        "call",
        "org.kde.kdeconnect",
        path,
        interface,
        method,
    ];
    args.extend_from_slice(extra);
    command_text("busctl", &args, Duration::from_secs(2)).await
}

async fn device_bool_property(device_id: &str, property: &str) -> AppletResult<Option<bool>> {
    let output = device_property(device_id, property).await?;
    Ok(parse_busctl_bool(&output))
}

async fn device_string_property(device_id: &str, property: &str) -> AppletResult<Option<String>> {
    let output = device_property(device_id, property).await?;
    Ok(parse_busctl_string(&output))
}

async fn device_property(device_id: &str, property: &str) -> AppletResult<String> {
    command_text(
        "busctl",
        &[
            "--user",
            "get-property",
            "org.kde.kdeconnect",
            &device_path(device_id),
            "org.kde.kdeconnect.device",
            property,
        ],
        Duration::from_secs(2),
    )
    .await
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

fn device_path(device_id: &str) -> String {
    format!(
        "/modules/kdeconnect/devices/{}",
        device_id.replace('-', "_")
    )
}

fn parse_charge(output: &str) -> Option<u8> {
    let value = output.split_whitespace().nth(1)?.parse::<u8>().ok()?;
    (value <= 100).then_some(value)
}

fn parse_busctl_bool(output: &str) -> Option<bool> {
    let mut parts = output.split_whitespace();
    if parts.next()? != "b" {
        return None;
    }
    match parts.next()? {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

async fn watch_phone_notifications(tx: mpsc::Sender<Msg>) -> AppletResult<()> {
    eprintln!("kdeconnect notification bridge: connecting to session bus");
    let connection = match zbus::Connection::session().await {
        Ok(connection) => {
            eprintln!("kdeconnect notification bridge: connected to session bus");
            connection
        }
        Err(err) => {
            eprintln!("kdeconnect notification bridge: session bus connection failed: {err}");
            return Err(err.into());
        }
    };
    eprintln!("kdeconnect notification bridge: creating notifications signal match rule");
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender("org.kde.kdeconnect")?
        .interface("org.kde.kdeconnect.device.notifications")?
        .build()
        .into_owned();
    let mut stream = match MessageStream::for_match_rule(rule, &connection, None).await {
        Ok(stream) => {
            eprintln!("kdeconnect notification bridge: subscribed to notification signals");
            stream
        }
        Err(err) => {
            eprintln!("kdeconnect notification bridge: signal subscription failed: {err}");
            return Err(err.into());
        }
    };

    while let Some(message) = stream.next().await {
        let message = match message {
            Ok(message) => message,
            Err(err) => {
                eprintln!("kdeconnect notification bridge: failed to read signal message: {err}");
                return Err(err.into());
            }
        };
        let Some(member) = message.header().member().map(|member| member.to_string()) else {
            eprintln!("kdeconnect notification bridge: skipping signal without member");
            continue;
        };
        let Some(path) = message.header().path().map(|path| path.to_string()) else {
            eprintln!(
                "kdeconnect notification bridge: skipping signal {member} without object path"
            );
            continue;
        };
        eprintln!("kdeconnect notification bridge: signal member={member} path={path}");
        let Some(device_id) = device_id_from_notifications_path(&path) else {
            eprintln!(
                "kdeconnect notification bridge: skipping signal {member}; path does not match notifications object"
            );
            continue;
        };
        eprintln!("kdeconnect notification bridge: parsed device_id={device_id}");

        match member.as_str() {
            "notificationPosted" | "notificationUpdated" => {
                let remote_id = match notification_signal_id(&message) {
                    Ok(remote_id) => {
                        eprintln!("kdeconnect notification bridge: parsed remote_id={remote_id}");
                        remote_id
                    }
                    Err(err) => {
                        eprintln!(
                            "kdeconnect notification bridge: failed to parse {member} body: {err}"
                        );
                        continue;
                    }
                };
                match phone_notification_by_device_id(&device_id, &remote_id).await {
                    Some(notification) => {
                        eprintln!(
                            "kdeconnect notification bridge: loaded notification key={} app={} summary={}",
                            notification.key, notification.app_name, notification.summary
                        );
                        if tx
                            .send(Msg::MirrorNotification(notification))
                            .await
                            .is_err()
                        {
                            eprintln!(
                                "kdeconnect notification bridge: applet channel closed while mirroring notification"
                            );
                            break;
                        }
                        eprintln!(
                            "kdeconnect notification bridge: queued desktop notification mirror"
                        );
                    }
                    None => {
                        eprintln!(
                            "kdeconnect notification bridge: notification properties unavailable for device_id={device_id} remote_id={remote_id}"
                        );
                    }
                }
            }
            "notificationRemoved" => {
                let remote_id = match notification_signal_id(&message) {
                    Ok(remote_id) => {
                        eprintln!(
                            "kdeconnect notification bridge: parsed removed remote_id={remote_id}"
                        );
                        remote_id
                    }
                    Err(err) => {
                        eprintln!(
                            "kdeconnect notification bridge: failed to parse {member} body: {err}"
                        );
                        continue;
                    }
                };
                let key = notification_key(&device_id, &remote_id);
                eprintln!(
                    "kdeconnect notification bridge: queued desktop notification removal key={key}"
                );
                if tx.send(Msg::RemoveNotification(key)).await.is_err() {
                    eprintln!(
                        "kdeconnect notification bridge: applet channel closed while removing notification"
                    );
                    break;
                }
            }
            "allNotificationsRemoved" => {
                eprintln!(
                    "kdeconnect notification bridge: queued removal for all device notifications device_id={device_id}"
                );
                let removed = tx.send(Msg::RemoveDeviceNotifications(device_id)).await;
                if removed.is_err() {
                    eprintln!(
                        "kdeconnect notification bridge: applet channel closed while removing device notifications"
                    );
                    break;
                }
            }
            _ => {
                eprintln!(
                    "kdeconnect notification bridge: ignoring notification signal member={member}"
                );
            }
        }
    }

    Ok(())
}

fn notification_signal_id(message: &zbus::Message) -> zbus::Result<String> {
    message.body().deserialize::<String>()
}

fn device_id_from_notifications_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/modules/kdeconnect/devices/")?;
    let (segment, tail) = rest.split_once("/")?;
    (tail == "notifications").then(|| segment.replace("_", "-"))
}

async fn phone_notification_by_device_id(
    device_id: &str,
    remote_id: &str,
) -> Option<KdeNotification> {
    let name = match device_string_property(device_id, "name").await {
        Ok(Some(name)) => {
            eprintln!(
                "kdeconnect notification bridge: loaded device name device_id={device_id} name={name}"
            );
            name
        }
        Ok(None) => {
            eprintln!(
                "kdeconnect notification bridge: device name unavailable for device_id={device_id}; using fallback"
            );
            "Phone".into()
        }
        Err(err) => {
            eprintln!(
                "kdeconnect notification bridge: failed to load device name device_id={device_id}: {err}; using fallback"
            );
            "Phone".into()
        }
    };
    let device = Device::new(device_id, name);
    phone_notification(&device, remote_id).await
}

async fn phone_notification(device: &Device, remote_id: &str) -> Option<KdeNotification> {
    let path = notification_path(&device.id, remote_id);
    eprintln!("kdeconnect notification bridge: loading notification properties path={path}");
    let app_name = kde_notification_property(&path, "appName")
        .await
        .unwrap_or_else(|| {
            eprintln!(
                "kdeconnect notification bridge: appName unavailable path={path}; using device name"
            );
            device.name.clone()
        });
    let ticker = kde_notification_property(&path, "ticker").await;
    let summary = kde_notification_property(&path, "title")
        .await
        .or_else(|| ticker.clone())
        .unwrap_or_else(|| "Phone notification".into());
    let body = kde_notification_property(&path, "text")
        .await
        .or(ticker)
        .unwrap_or_default();
    let icon = kde_notification_property(&path, "iconPath").await;

    Some(KdeNotification {
        key: notification_key(&device.id, remote_id),
        device_name: device.name.clone(),
        app_name,
        summary,
        body,
        icon,
    })
}

async fn kde_notification_property(path: &str, property: &str) -> Option<String> {
    let output = command_output(
        "busctl",
        &[
            "--user",
            "get-property",
            "org.kde.kdeconnect",
            path,
            "org.kde.kdeconnect.device.notifications.notification",
            property,
        ],
        Duration::from_secs(2),
    )
    .await;

    let output = match output {
        Ok(output) => output,
        Err(err) => {
            eprintln!(
                "kdeconnect notification bridge: failed to read notification property path={path} property={property}: {err}"
            );
            return None;
        }
    };

    let value = parse_busctl_string(&String::from_utf8_lossy(&output.stdout));
    match &value {
        Some(value) => eprintln!(
            "kdeconnect notification bridge: property path={path} property={property} value={value}"
        ),
        None => eprintln!(
            "kdeconnect notification bridge: property path={path} property={property} empty or unparsable"
        ),
    }
    value
}

async fn mirror_desktop_notification(
    notification: &KdeNotification,
    replaces_id: u32,
) -> AppletResult<u32> {
    let mut args = vec![
        "--print-id".to_owned(),
        "--app-name".to_owned(),
        format!("KDE Connect - {}", notification.device_name),
    ];
    if replaces_id != 0 {
        args.push("--replace-id".to_owned());
        args.push(replaces_id.to_string());
    }
    if let Some(icon) = &notification.icon
        && !icon.trim().is_empty()
    {
        args.push("--icon".to_owned());
        args.push(icon.clone());
    }

    args.push(notification.summary.clone());
    args.push(notification.body.clone());

    eprintln!(
        "kdeconnect notification bridge: sending notify-send summary={} replaces_id={replaces_id}",
        notification.summary
    );
    let output = command_output_owned("notify-send", &args, Duration::from_secs(5)).await?;
    let id = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()?;
    eprintln!("kdeconnect notification bridge: notify-send returned desktop_id={id}");
    Ok(id)
}

async fn close_desktop_notification(id: u32) -> AppletResult<()> {
    let args = vec![
        "--user".to_owned(),
        "call".to_owned(),
        "org.freedesktop.Notifications".to_owned(),
        "/org/freedesktop/Notifications".to_owned(),
        "org.freedesktop.Notifications".to_owned(),
        "CloseNotification".to_owned(),
        "u".to_owned(),
        id.to_string(),
    ];
    command_output_owned("busctl", &args, Duration::from_secs(2)).await?;
    Ok(())
}

fn notification_key(device_id: &str, remote_id: &str) -> String {
    format!("{device_id}:{remote_id}")
}

fn notification_path(device_id: &str, remote_id: &str) -> String {
    if remote_id.starts_with('/') {
        return remote_id.to_owned();
    }
    format!(
        "{}/notifications/{}",
        device_path(device_id),
        object_path_segment(remote_id)
    )
}

fn object_path_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn parse_busctl_string(output: &str) -> Option<String> {
    parse_quoted_strings(output).into_iter().next()
}

fn parse_quoted_strings(output: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut chars = output.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '"' {
            continue;
        }

        let mut value = String::new();
        let mut escaped = false;
        for next in chars.by_ref() {
            if escaped {
                value.push(match next {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    '"' => '"',
                    '\\' => '\\',
                    other => other,
                });
                escaped = false;
            } else if next == '\\' {
                escaped = true;
            } else if next == '"' {
                break;
            } else {
                value.push(next);
            }
        }
        values.push(value);
    }
    values
}

async fn command_output(
    program: &str,
    args: &[&str],
    limit: Duration,
) -> AppletResult<std::process::Output> {
    let mut command = prepare_command(program);
    command.args(args);

    run_command_output(program, command, limit).await
}

async fn command_output_owned(
    program: &str,
    args: &[String],
    limit: Duration,
) -> AppletResult<std::process::Output> {
    let mut command = prepare_command(program);
    command.args(args);

    run_command_output(program, command, limit).await
}

fn prepare_command(program: &str) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(program);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
}

async fn run_command_output(
    program: &str,
    mut command: tokio::process::Command,
    limit: Duration,
) -> AppletResult<std::process::Output> {
    let output = match tokio::time::timeout(limit, command.output()).await {
        Ok(output) => output?,
        Err(_) => return Err(format!("{program} timed out after {limit:?}").into()),
    };

    if output.status.success() {
        return Ok(output);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.is_empty() {
        Err(format!("{program} exited with {}", output.status).into())
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
    fn parses_device_paths_from_busctl_tree() {
        let paths = parse_device_paths(
            "├─ /modules/kdeconnect/devices/phone_one\n│ ├─ /modules/kdeconnect/devices/phone_one/battery\n└─ /modules/kdeconnect/devices/deck_two\n",
        );

        assert_eq!(paths, vec!["phone-one".to_owned(), "deck-two".to_owned()]);
    }

    #[test]
    fn reconcile_expanded_prunes_missing_devices_without_expanding_by_default() {
        let mut state = State {
            devices: vec![
                Device::new("phone", "Phone"),
                Device::new("deck", "Steam Deck"),
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
            devices: vec![Device::new("phone", "Phone")],
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

    #[test]
    fn parses_busctl_bool_values() {
        assert_eq!(parse_busctl_bool("b true\n"), Some(true));
        assert_eq!(parse_busctl_bool("b false\n"), Some(false));
        assert_eq!(parse_busctl_bool("s true\n"), None);
    }

    #[test]
    fn parses_notification_signal_device_paths() {
        assert_eq!(
            device_id_from_notifications_path(
                "/modules/kdeconnect/devices/phone_one/notifications"
            ),
            Some("phone-one".to_owned())
        );
        assert_eq!(
            device_id_from_notifications_path("/modules/kdeconnect/devices/phone_one/battery"),
            None
        );
        assert_eq!(device_id_from_notifications_path("/other"), None);
    }

    #[test]
    fn notification_paths_sanitize_remote_ids() {
        assert_eq!(
            notification_path("phone-id", "app:42"),
            "/modules/kdeconnect/devices/phone_id/notifications/app_42"
        );
        assert_eq!(
            notification_path("phone-id", "/custom/path"),
            "/custom/path"
        );
    }

    #[tokio::test]
    async fn status_is_only_visible_when_devices_are_connected() {
        let empty = KdeConnectApplet.status(&State::default()).await.unwrap();
        assert!(empty.is_empty());

        let connected = KdeConnectApplet
            .status(&State {
                devices: vec![Device::new("phone", "Phone")],
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
        let mut device = Device::new("phone", "Pixel 8 Pro");
        device.charge = Some(87);
        let state = State {
            devices: vec![device],
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
        assert_eq!(tile["data"]["right"]["type"], "label");
        assert_eq!(tile["data"]["right"]["data"]["label"], "87%");
        assert_eq!(
            tile["data"]["right"]["data"]["css_classes"],
            json!(["caption", "numeric"])
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
                    }
                },
                {
                    "type": "tile",
                    "data": {
                        "id": "ring-phone",
                        "primary": "Find device",
                        "left_icon": "audio-volume-high-symbolic",
                    }
                },
                {
                    "type": "tile",
                    "data": {
                        "id": "browse-phone",
                        "primary": "Browse files",
                        "left_icon": "folder-symbolic",
                    }
                },
                {
                    "type": "tile",
                    "data": {
                        "id": "clipboard-phone",
                        "primary": "Send clipboard to phone",
                        "left_icon": "edit-paste-symbolic",
                    }
                }
            ])
        );
    }
}
