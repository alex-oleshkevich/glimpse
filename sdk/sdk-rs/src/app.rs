use async_trait::async_trait;
use serde::Serialize;
use std::process::Stdio;
use tokio::{
    io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
};

use crate::{
    events::{
        CallbackEvent, InitEvent, parse_callback_event, parse_incoming_line, parse_init_event,
    },
    protocol::StatusItem,
    widgets::TreeNode,
};

pub type AppletError = Box<dyn std::error::Error + Send + Sync>;
pub type AppletResult<T> = Result<T, AppletError>;

/// An exec applet. The applet itself is a stateless method bag — state
/// lives in `Self::State` and is passed in by the runtime to every
/// method. Mutate the state directly inside handlers and the next render
/// sees the new value.
///
/// `status` and `popover` are pure functions of `state`; they should not
/// mutate. The runtime calls them after every event and emits a wire
/// message only when the output changes.
#[async_trait]
pub trait Applet: Send + Sync {
    type State: Send + Sync + 'static;

    /// Build the panel status items for the current state.
    async fn status(&self, state: &Self::State) -> AppletResult<Vec<StatusItem>>;

    /// Build the popover content tree, or `None` for no popover.
    /// The default impl returns `None` so applets without popovers
    /// don't have to implement this.
    async fn popover(&self, _state: &Self::State) -> AppletResult<Option<TreeNode>> {
        Ok(None)
    }

    /// Called once before the read loop begins.
    async fn on_start(&mut self, _state: &mut Self::State) -> AppletResult<()> {
        Ok(())
    }

    /// Called once when Glimpse sends the `init` line.
    async fn on_init(&mut self, _state: &mut Self::State, _event: InitEvent) -> AppletResult<()> {
        Ok(())
    }

    /// Called for every interactive event (click, scroll, toggle, change,
    /// popover open/close, etc.).
    async fn on_callback(
        &mut self,
        _state: &mut Self::State,
        _event: CallbackEvent,
    ) -> AppletResult<()> {
        Ok(())
    }

    /// CSS class applied to the applet indicator and popover (e.g. `"workstation"`
    /// → `applet-workstation` is added to both GTK widgets). Return `None` (the
    /// default) to leave styling to the global theme.
    fn css_class(&self) -> Option<&str> {
        None
    }

    /// Write a debug line to stderr. In `applets dev` mode the line appears
    /// directly in the terminal; when running under the panel it is captured
    /// by the shell's stderr logger.
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
    stdout.write_all(b"close_popover {}\n").await?;
    stdout.flush().await?;
    Ok(())
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
    let output = Command::new(command[0]).args(&command[1..]).output().await?;
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
struct TreePayload {
    root: Option<TreeNode>,
}

struct LastSeen {
    status: Vec<StatusItem>,
    tree: Option<TreeNode>,
    initialized: bool,
}

impl LastSeen {
    fn new() -> Self {
        Self {
            status: Vec::new(),
            tree: None,
            initialized: false,
        }
    }
}

/// Run an applet against stdin/stdout, owning the state for its lifetime.
pub async fn run<A>(mut applet: A, mut state: A::State) -> AppletResult<()>
where
    A: Applet,
{
    let mut stdout = io::stdout();
    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();
    let mut last = LastSeen::new();
    applet.on_start(&mut state).await?;
    if let Some(class) = applet.css_class() {
        stdout
            .write_all(format!("class {class}\n").as_bytes())
            .await?;
        stdout.flush().await?;
    }
    flush(&mut stdout, &applet, &state, &mut last).await?;

    while let Some(line) = lines.next_line().await? {
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
        let result: AppletResult<()> = match incoming.kind.as_str() {
            "init" => match parse_init_event(incoming.data) {
                Ok(evt) => applet.on_init(&mut state, evt).await,
                Err(err) => {
                    eprintln!("glimpse-sdk: ignoring malformed init: {err}");
                    continue;
                }
            },
            "event" => match parse_callback_event(incoming.data) {
                Ok(event) => applet.on_callback(&mut state, event).await,
                Err(err) => {
                    eprintln!("glimpse-sdk: ignoring malformed event: {err}");
                    continue;
                }
            },
            _ => continue,
        };
        result?;

        flush(&mut stdout, &applet, &state, &mut last).await?;
    }

    Ok(())
}

async fn flush<A>(
    stdout: &mut io::Stdout,
    applet: &A,
    state: &A::State,
    last: &mut LastSeen,
) -> AppletResult<()>
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
    Ok(())
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
    use crate::{Badge, Button, CallbackEvent, ClickEvent, Column, StatusItem, Text, TreeNode};

    struct DemoApplet;

    #[derive(Debug, Clone)]
    struct DemoState {
        version: String,
        clicks: u32,
    }

    #[async_trait]
    impl Applet for DemoApplet {
        type State = DemoState;

        async fn status(&self, state: &Self::State) -> AppletResult<Vec<StatusItem>> {
            Ok(vec![
                StatusItem::new("demo")
                    .icon("demo-symbolic")
                    .label(state.version.clone()),
            ])
        }

        async fn popover(&self, state: &Self::State) -> AppletResult<Option<TreeNode>> {
            Ok(Some(TreeNode::from(Column::new(vec![
                TreeNode::from(crate::Hero::new("Demo", state.version.clone())),
                TreeNode::from(Text::new(state.version.clone())),
                TreeNode::from(Button::new("submit").label("Submit")),
            ]))))
        }

        async fn on_callback(
            &mut self,
            state: &mut Self::State,
            event: CallbackEvent,
        ) -> AppletResult<()> {
            if let CallbackEvent::Click(click) = event {
                if click.id == "submit" {
                    state.clicks += 1;
                    state.version = "v2".into();
                }
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
            CallbackEvent::Click(ClickEvent {
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
    fn select_tree_nodes_serialize() {
        let node = crate::Select::new("env", vec![("prod".into(), "Production".into())]);
        let payload = serde_json::to_value(TreeNode::from(node)).expect("tree should serialize");
        assert_eq!(payload["type"], "select");
        assert_eq!(payload["data"]["items"][0]["id"], "prod");
    }

    #[test]
    fn status_dot_serializes_as_status_protocol_name() {
        let payload = serde_json::to_value(TreeNode::from(crate::StatusDot::new()))
            .expect("status dot serializes");
        assert_eq!(payload["type"], "status");
    }

    #[test]
    fn row_and_column_serialize_as_layout_protocol_names() {
        let row =
            serde_json::to_value(TreeNode::from(crate::Row::new(vec![]))).expect("row serializes");
        assert_eq!(row["type"], "row");

        let column = serde_json::to_value(TreeNode::from(crate::Column::new(vec![])))
            .expect("column serializes");
        assert_eq!(column["type"], "column");
    }

    #[test]
    fn spinner_serializes_with_default_spinning() {
        let payload = serde_json::to_value(TreeNode::from(crate::Spinner::new()))
            .expect("spinner serializes");
        assert_eq!(payload["type"], "spinner");
        assert_eq!(payload["data"]["spinning"], true);
    }

    #[test]
    fn variant_serializes_as_semantic_protocol_value() {
        let mut badge = Badge::new("Warning");
        badge.variant = Some(crate::Variant::Warning);
        let payload = serde_json::to_value(TreeNode::from(badge)).expect("tree should serialize");
        assert_eq!(payload["data"]["variant"], "warning");
    }

    #[tokio::test]
    async fn callback_mutates_state_and_status_observes_it() {
        let mut applet = DemoApplet;
        let mut state = DemoState {
            version: "v1".into(),
            clicks: 0,
        };

        applet
            .on_callback(
                &mut state,
                CallbackEvent::Click(ClickEvent {
                    id: "submit".into(),
                    button: Some("left".into()),
                }),
            )
            .await
            .expect("callback should update state");

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

    #[tokio::test]
    async fn run_command_returns_stdout_stderr_and_rc() {
        let result = run_command(&[
            "sh",
            "-c",
            "printf 'out\\n'; printf 'err\\n' >&2; exit 7",
        ])
        .await
        .expect("command should run");

        assert_eq!(result.stdout, "out\n");
        assert_eq!(result.stderr, "err\n");
        assert_eq!(result.rc, 7);
    }

    #[tokio::test]
    async fn run_command_rejects_empty_command() {
        let err = run_command(&[]).await.expect_err("empty command should fail");

        assert!(err.to_string().contains("command must not be empty"));
    }
}
