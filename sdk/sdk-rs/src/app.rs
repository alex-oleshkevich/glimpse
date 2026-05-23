use async_trait::async_trait;
use serde::Serialize;
use std::collections::HashMap;
use std::process::Stdio;
use tokio::{
    io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::mpsc,
};

use crate::{
    AppletResult,
    events::{
        CallbackEvent, InitEvent, InputEvent, PopoverEvent, ScrollEvent, parse_callback_event,
        parse_incoming_line, parse_init_event,
    },
    protocol::StatusItem,
    widgets::TreeNode,
};

#[async_trait]
pub trait Applet: Send + Sync {
    type State: Clone + Send + Sync + 'static;
    type Msg: Clone + PartialEq + Send + 'static;

    async fn status(&self, state: &Self::State) -> AppletResult<Vec<StatusItem>>;

    async fn popover(&self, _state: &Self::State) -> AppletResult<Option<TreeNode<Self::Msg>>> {
        Ok(None)
    }

    /// Called when a widget emits a message. All state mutation lives here.
    async fn update(&mut self, _state: &mut Self::State, _msg: Self::Msg) -> AppletResult<()> {
        Ok(())
    }

    async fn on_start(
        &mut self,
        _state: &mut Self::State,
        _tx: mpsc::Sender<Self::Msg>,
    ) -> AppletResult<()> {
        Ok(())
    }

    async fn on_init(&mut self, _state: &mut Self::State, _event: InitEvent) -> AppletResult<()> {
        Ok(())
    }

    async fn on_scroll(
        &mut self,
        _state: &mut Self::State,
        _event: ScrollEvent,
    ) -> AppletResult<()> {
        Ok(())
    }

    async fn on_input(&mut self, _state: &mut Self::State, _event: InputEvent) -> AppletResult<()> {
        Ok(())
    }

    async fn on_popover(
        &mut self,
        _state: &mut Self::State,
        _event: PopoverEvent,
    ) -> AppletResult<()> {
        Ok(())
    }

    fn css_class(&self) -> Option<&str> {
        None
    }

    fn log(&self, msg: impl std::fmt::Display) {
        eprintln!("{msg}");
    }

    async fn run_command(&self, command: &[&str]) -> AppletResult<CommandResult> {
        run_command(command).await
    }

    async fn close_popover(&self) -> AppletResult<()> {
        close_popover().await
    }

    async fn copy_to_clipboard(&self, text: &str) -> AppletResult<()> {
        copy_to_clipboard(text).await
    }

    async fn open_uri(&self, uri: &str) -> AppletResult<()> {
        open_uri(uri).await
    }

    async fn show_notification(&self, summary: &str, body: Option<&str>) -> AppletResult<()> {
        show_notification(summary, body).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub rc: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DesktopCommand {
    program: String,
    args: Vec<String>,
    stdin: Option<String>,
}

pub async fn close_popover() -> AppletResult<()> {
    let mut stdout = io::stdout();
    stdout.write_all(close_popover_command()).await?;
    stdout.flush().await?;
    Ok(())
}

fn close_popover_command() -> &'static [u8] {
    b"close-popover\n"
}

pub async fn copy_to_clipboard(text: &str) -> AppletResult<()> {
    run_desktop_command(desktop_command_for_copy_to_clipboard(text)).await
}

pub async fn open_uri(uri: &str) -> AppletResult<()> {
    run_desktop_command(desktop_command_for_open_uri(uri)).await
}

pub async fn show_notification(summary: &str, body: Option<&str>) -> AppletResult<()> {
    run_desktop_command(desktop_command_for_show_notification(summary, body)).await
}

pub async fn run_command(command: &[&str]) -> AppletResult<CommandResult> {
    if command.is_empty() {
        return Err("command must not be empty".into());
    }
    let output = Command::new(command[0])
        .args(&command[1..])
        .output()
        .await?;
    Ok(CommandResult {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        rc: output.status.code().unwrap_or(-1),
    })
}

fn desktop_command_for_copy_to_clipboard(text: &str) -> DesktopCommand {
    DesktopCommand {
        program: "wl-copy".into(),
        args: vec![],
        stdin: Some(text.into()),
    }
}

fn desktop_command_for_open_uri(uri: &str) -> DesktopCommand {
    DesktopCommand {
        program: "xdg-open".into(),
        args: vec![uri.into()],
        stdin: None,
    }
}

fn desktop_command_for_show_notification(summary: &str, body: Option<&str>) -> DesktopCommand {
    let mut args = vec![summary.into()];
    if let Some(body) = body {
        args.push(body.into());
    }
    DesktopCommand {
        program: "notify-send".into(),
        args,
        stdin: None,
    }
}

async fn run_desktop_command(command: DesktopCommand) -> AppletResult<()> {
    let mut child = Command::new(&command.program)
        .args(&command.args)
        .stdin(if command.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    if let Some(input) = command.stdin {
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(input.as_bytes()).await?;
        }
    }

    let status = child.wait().await?;
    if !status.success() {
        return Err(format!("{} exited with status {status}", command.program).into());
    }
    Ok(())
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(bound(serialize = ""))]
struct TreePayload<Msg> {
    root: Option<TreeNode<Msg>>,
}

struct LastSeen<Msg> {
    status: Vec<StatusItem>,
    tree: Option<TreeNode<Msg>>,
    initialized: bool,
}

impl<Msg> LastSeen<Msg> {
    fn new() -> Self {
        Self {
            status: Vec::new(),
            tree: None,
            initialized: false,
        }
    }
}

// Maps widget IDs to messages extracted from the current widget tree.
struct MsgMap<Msg> {
    click: HashMap<String, Msg>,
    toggle: HashMap<String, std::sync::Arc<dyn Fn(bool) -> Msg + Send + Sync>>,
    change: HashMap<String, std::sync::Arc<dyn Fn(Option<serde_json::Value>) -> Msg + Send + Sync>>,
    status_ids: std::collections::HashSet<String>,
}

impl<Msg> MsgMap<Msg> {
    fn new() -> Self {
        Self {
            click: HashMap::new(),
            toggle: HashMap::new(),
            change: HashMap::new(),
            status_ids: std::collections::HashSet::new(),
        }
    }
}

fn collect_messages<Msg: Clone + 'static>(tree: &TreeNode<Msg>, map: &mut MsgMap<Msg>) {
    use crate::widgets::TreeNode::*;
    match tree {
        Hero(hero) => {
            if let Some(mapper) = &hero.on_toggle {
                if let Some(id) = &hero.id {
                    map.toggle
                        .insert(id.clone(), std::sync::Arc::clone(&mapper.0));
                }
            }
            if let Some(trailing) = &hero.trailing {
                collect_messages(trailing, map);
            }
        }
        PanelIndicator(indicator) => {
            if let (Some(id), Some(mapper)) = (&indicator.id, &indicator.on_click) {
                map.click.insert(id.clone(), mapper.map(()));
            }
            if let Some(extra) = &indicator.extra {
                collect_messages(extra, map);
            }
        }
        Tile(tile) => {
            if let (Some(id), Some(mapper)) = (&tile.id, &tile.on_click) {
                map.click.insert(id.clone(), mapper.map(()));
            }
            if let Some(left) = &tile.left {
                collect_messages(left, map);
            }
            if let Some(right) = &tile.right {
                collect_messages(right, map);
            }
        }
        SegmentedTile(tile) => {
            if let (Some(id), Some(mapper)) = (&tile.id, &tile.on_click) {
                map.click.insert(id.clone(), mapper.map(()));
            }
            if let (Some(id), Some(mapper)) = (&tile.id, &tile.on_toggle) {
                map.toggle
                    .insert(id.clone(), std::sync::Arc::clone(&mapper.0));
            }
            if let Some(left) = &tile.left {
                collect_messages(left, map);
            }
            if let Some(right) = &tile.right {
                collect_messages(right, map);
            }
            if let Some(child) = &tile.child {
                collect_messages(child, map);
            }
        }
        SwitchTile(tile) => {
            if let Some(mapper) = &tile.on_toggle {
                map.toggle
                    .insert(tile.id.clone(), std::sync::Arc::clone(&mapper.0));
            }
            if let Some(left) = &tile.left {
                collect_messages(left, map);
            }
        }
        ExpanderTile(tile) => {
            if let (Some(id), Some(mapper)) = (&tile.id, &tile.on_toggle) {
                map.toggle
                    .insert(id.clone(), std::sync::Arc::clone(&mapper.0));
            }
            if let Some(left) = &tile.left {
                collect_messages(left, map);
            }
            if let Some(child) = &tile.child {
                collect_messages(child, map);
            }
        }
        SliderTile(tile) => {
            if let Some(mapper) = &tile.on_change {
                let mapper = mapper.clone();
                map.change.insert(
                    tile.id.clone(),
                    std::sync::Arc::new(move |value| {
                        mapper.map(value.and_then(|v| v.as_f64()).unwrap_or_default())
                    }),
                );
            }
            if let Some(left) = &tile.left {
                collect_messages(left, map);
            }
        }
        Meter(meter) => {
            if let (Some(id), Some(mapper)) = (&meter.id, &meter.on_change) {
                let mapper = mapper.clone();
                map.change.insert(
                    id.clone(),
                    std::sync::Arc::new(move |value| {
                        mapper.map(value.and_then(|v| v.as_f64()).unwrap_or_default())
                    }),
                );
            }
        }
        ChoiceTile(tile) => {
            if let (Some(id), Some(mapper)) = (&tile.id, &tile.on_click) {
                map.click.insert(id.clone(), mapper.map(()));
            }
            if let Some(left) = &tile.left {
                collect_messages(left, map);
            }
        }
        ChoiceList(list) => {
            if let Some(mapper) = &list.on_change {
                let mapper = mapper.clone();
                map.change.insert(
                    list.id.clone(),
                    std::sync::Arc::new(move |value| {
                        mapper.map(
                            value
                                .and_then(|v| v.as_str().map(ToOwned::to_owned))
                                .unwrap_or_default(),
                        )
                    }),
                );
            }
        }
        PagerItem(item) => {
            if let Some(mapper) = &item.on_click {
                map.click.insert(item.id.to_string(), mapper.map(()));
            }
        }
        PagerStrip(strip) => {
            if let (Some(id), Some(mapper)) = (&strip.id, &strip.on_change) {
                let mapper = mapper.clone();
                map.change.insert(
                    id.clone(),
                    std::sync::Arc::new(move |value| {
                        mapper.map(value.and_then(|v| v.as_u64()).unwrap_or_default())
                    }),
                );
            }
            for item in &strip.items {
                collect_messages(&TreeNode::PagerItem(item.clone()), map);
            }
        }
        Calendar(calendar) => {
            if let (Some(id), Some(mapper)) = (&calendar.id, &calendar.on_change) {
                let mapper = mapper.clone();
                map.change.insert(
                    id.clone(),
                    std::sync::Arc::new(move |value| {
                        mapper.map(
                            value
                                .and_then(|v| v.as_str().map(ToOwned::to_owned))
                                .unwrap_or_default(),
                        )
                    }),
                );
            }
        }
        Row(row) => {
            for child in &row.children {
                collect_messages(child, map);
            }
        }
        Column(col) => {
            for child in &col.children {
                collect_messages(child, map);
            }
        }
        Container(c) => {
            for child in &c.children {
                collect_messages(child, map);
            }
        }
        BoxedList(list) => {
            for child in &list.children {
                collect_messages(child, map);
            }
        }
        ButtonRow(row) => {
            for child in &row.children {
                collect_messages(child, map);
            }
        }
        PopoverShell(shell) => {
            for child in &shell.children {
                collect_messages(child, map);
            }
            for child in &shell.footer {
                collect_messages(child, map);
            }
        }
        Scroll(scroll) => {
            collect_messages(&scroll.child, map);
        }
        _ => {}
    }
}

async fn dispatch_msg<A: Applet>(
    map: &MsgMap<A::Msg>,
    applet: &mut A,
    state: &mut A::State,
    event: CallbackEvent,
) -> AppletResult<()> {
    let msg = match &event {
        CallbackEvent::Click(e) => map.click.get(&e.id).cloned(),
        CallbackEvent::Toggle(e) => map.toggle.get(&e.id).map(|f| f(e.value)),
        CallbackEvent::Change(e) => map.change.get(&e.id).map(|f| f(e.value.clone())),
        _ => None,
    };

    if let Some(msg) = msg {
        applet.update(state, msg).await?;
        return Ok(());
    }

    // Fallback for events without a registered message
    match event {
        CallbackEvent::Scroll(e) => applet.on_scroll(state, e).await?,
        CallbackEvent::Input(e) => applet.on_input(state, e).await?,
        CallbackEvent::Popover(e) => applet.on_popover(state, e).await?,
        CallbackEvent::Click(e) => {
            if !map.status_ids.contains(&e.id) {
                eprintln!("glimpse-sdk: unhandled click event for id {:?}", e.id);
            }
        }
        CallbackEvent::Toggle(e) => {
            eprintln!("glimpse-sdk: unhandled toggle event for id {:?}", e.id);
        }
        CallbackEvent::Change(e) => {
            eprintln!("glimpse-sdk: unhandled change event for id {:?}", e.id);
        }
    }
    Ok(())
}

pub async fn run<A>(mut applet: A, mut state: A::State) -> AppletResult<()>
where
    A: Applet,
{
    let mut stdout = io::stdout();
    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();
    let mut last = LastSeen::new();
    let (tx, mut rx) = mpsc::channel::<A::Msg>(32);
    applet.on_start(&mut state, tx).await?;
    if let Some(class) = applet.css_class() {
        stdout
            .write_all(format!("class {class}\n").as_bytes())
            .await?;
        stdout.flush().await?;
    }
    let mut msg_map = flush(&mut stdout, &applet, &state, &mut last).await?;

    loop {
        let result: AppletResult<Option<()>> = tokio::select! {
            line = lines.next_line() => {
                let Some(line) = line? else { break };
                if line.trim().is_empty() {
                    continue;
                }
                let incoming = match parse_incoming_line(&line) {
                    Ok(msg) => msg,
                    Err(err) => {
                        eprintln!("glimpse-sdk: ignoring malformed input: {err}");
                        continue;
                    }
                };
                match incoming.kind.as_str() {
                    "init" => match parse_init_event(incoming.data) {
                        Ok(evt) => applet.on_init(&mut state, evt).await.map(Some),
                        Err(err) => {
                            eprintln!("glimpse-sdk: ignoring malformed init: {err}");
                            continue;
                        }
                    },
                    "event" => match parse_callback_event(incoming.data) {
                        Ok(event) => dispatch_msg(&msg_map, &mut applet, &mut state, event).await.map(Some),
                        Err(err) => {
                            eprintln!("glimpse-sdk: ignoring malformed event: {err}");
                            continue;
                        }
                    },
                    _ => continue,
                }
            }
            Some(msg) = rx.recv() => {
                applet.update(&mut state, msg).await.map(Some)
            }
        };
        result?;
        msg_map = flush(&mut stdout, &applet, &state, &mut last).await?;
    }

    Ok(())
}

async fn flush<A>(
    stdout: &mut io::Stdout,
    applet: &A,
    state: &A::State,
    last: &mut LastSeen<A::Msg>,
) -> AppletResult<MsgMap<A::Msg>>
where
    A: Applet,
{
    let next_status = applet.status(state).await?;
    if !last.initialized || last.status != next_status {
        write_message(
            stdout,
            "status",
            &serde_json::json!({ "items": next_status }),
        )
        .await?;
        last.status = next_status;
    }

    let next_tree = applet.popover(state).await?;
    let mut map = MsgMap::new();
    for item in &last.status {
        if let Some(id) = &item.id {
            map.status_ids.insert(id.clone());
        }
    }
    if let Some(ref tree) = next_tree {
        collect_messages(tree, &mut map);
    }
    if last.tree != next_tree {
        write_message(
            stdout,
            "popover",
            &TreePayload {
                root: next_tree.clone(),
            },
        )
        .await?;
        last.tree = next_tree;
    }

    last.initialized = true;
    Ok(map)
}

async fn write_message<T: Serialize>(
    stdout: &mut io::Stdout,
    command: &str,
    payload: &T,
) -> AppletResult<()> {
    let encoded = serde_json::to_vec(payload)?;
    stdout.write_all(command.as_bytes()).await?;
    stdout.write_all(b" ").await?;
    stdout.write_all(&encoded).await?;
    stdout.write_all(b"\n").await?;
    stdout.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{
        Badge, BadgeKind, Choice, ChoiceList, Column, MsgMapper, StatusItem, Text, Tile, TreeNode,
    };

    // A minimal Msg type for tests that need interaction.
    #[derive(Debug, Clone, PartialEq)]
    enum DemoMsg {
        Submit,
    }

    struct DemoApplet;

    #[derive(Debug, Clone)]
    struct DemoState {
        version: String,
        clicks: u32,
    }

    #[async_trait]
    impl Applet for DemoApplet {
        type State = DemoState;
        type Msg = DemoMsg;

        async fn status(&self, state: &Self::State) -> AppletResult<Vec<StatusItem>> {
            Ok(vec![
                StatusItem::new("demo")
                    .icon("demo-symbolic")
                    .label(state.version.clone()),
            ])
        }

        async fn popover(&self, state: &Self::State) -> AppletResult<Option<TreeNode<Self::Msg>>> {
            Ok(Some(TreeNode::from(Column::new(vec![
                TreeNode::from(crate::Hero::new("Demo", state.version.clone())),
                TreeNode::from(Text::new(state.version.clone())),
                {
                    let mut tile = Tile::new("Submit");
                    tile.id = Some("submit".into());
                    tile.activatable = true;
                    tile.on_click = Some(MsgMapper::new(|()| DemoMsg::Submit));
                    TreeNode::from(tile)
                },
            ]))))
        }

        async fn update(&mut self, state: &mut Self::State, msg: Self::Msg) -> AppletResult<()> {
            if msg == DemoMsg::Submit {
                state.clicks += 1;
                state.version = "v2".into();
            }
            Ok(())
        }
    }

    #[test]
    fn parse_callback_event_returns_typed_click_variant() {
        let event = parse_callback_event(json!({
            "id": "submit",
            "type": "click",
            "button": "left"
        }))
        .expect("click event should parse");

        assert_eq!(
            event,
            CallbackEvent::Click(crate::ClickEvent {
                id: "submit".into(),
                button: Some("left".into()),
            })
        );
    }

    #[test]
    fn parse_callback_event_returns_typed_popover_variant() {
        let event = parse_callback_event(json!({
            "id": "popover",
            "type": "open",
            "source": "popover"
        }))
        .expect("popover event should parse");

        assert_eq!(
            event,
            CallbackEvent::Popover(crate::PopoverEvent { open: true })
        );
    }

    #[test]
    fn choice_list_tree_nodes_serialize() {
        let mut node = ChoiceList::<()>::new(
            "env",
            vec![Choice {
                id: "prod".into(),
                primary: "Production".into(),
                secondary: None,
                icon: None,
            }],
        );
        node.active = Some("prod".into());
        let payload =
            serde_json::to_value(TreeNode::<()>::from(node)).expect("tree should serialize");
        assert_eq!(payload["type"], "choice_list");
        assert_eq!(payload["data"]["choices"][0]["id"], "prod");
    }

    #[test]
    fn status_dot_serializes_as_status_protocol_name() {
        let payload = serde_json::to_value(TreeNode::<()>::from(crate::StatusDot::new()))
            .expect("status dot serializes");
        assert_eq!(payload["type"], "status_dot");
    }

    #[test]
    fn row_and_column_serialize_as_layout_protocol_names() {
        let row = serde_json::to_value(TreeNode::<()>::from(crate::Row::new(vec![])))
            .expect("row serializes");
        assert_eq!(row["type"], "row");

        let column = serde_json::to_value(TreeNode::<()>::from(crate::Column::new(vec![])))
            .expect("column serializes");
        assert_eq!(column["type"], "column");
    }

    #[test]
    fn variant_serializes_as_semantic_protocol_value() {
        let mut badge = Badge::new("Warning");
        badge.kind = BadgeKind::Warning;
        let payload =
            serde_json::to_value(TreeNode::<()>::from(badge)).expect("tree should serialize");
        assert_eq!(payload["data"]["kind"], "warning");
    }

    #[tokio::test]
    async fn update_mutates_state_and_status_observes_it() {
        let mut applet = DemoApplet;
        let mut state = DemoState {
            version: "v1".into(),
            clicks: 0,
        };

        applet
            .update(&mut state, DemoMsg::Submit)
            .await
            .expect("update should succeed");

        let status = applet.status(&state).await.expect("status should succeed");
        assert_eq!(status[0].label.as_deref(), Some("v2"));
        assert_eq!(state.clicks, 1);
    }

    #[test]
    fn desktop_helpers_build_local_commands() {
        assert_eq!(
            desktop_command_for_copy_to_clipboard("hello"),
            DesktopCommand {
                program: "wl-copy".into(),
                args: vec![],
                stdin: Some("hello".into()),
            }
        );
        assert_eq!(
            desktop_command_for_open_uri("https://example.com"),
            DesktopCommand {
                program: "xdg-open".into(),
                args: vec!["https://example.com".into()],
                stdin: None,
            }
        );
        assert_eq!(
            desktop_command_for_show_notification("Build complete", Some("Tests passed")),
            DesktopCommand {
                program: "notify-send".into(),
                args: vec!["Build complete".into(), "Tests passed".into()],
                stdin: None,
            }
        );
    }

    #[test]
    fn close_popover_command_matches_shell_protocol() {
        assert_eq!(close_popover_command(), b"close-popover\n");
    }

    #[tokio::test]
    async fn run_command_returns_stdout_stderr_and_rc() {
        let result = run_command(&["sh", "-c", "printf 'out\\n'; printf 'err\\n' >&2; exit 7"])
            .await
            .expect("command should run");

        assert_eq!(result.stdout, "out\n");
        assert_eq!(result.stderr, "err\n");
        assert_eq!(result.rc, 7);
    }

    #[tokio::test]
    async fn run_command_rejects_empty_command() {
        let err = run_command(&[])
            .await
            .expect_err("empty command should fail");
        assert!(err.to_string().contains("command must not be empty"));
    }
}
