mod event;
#[cfg(test)]
mod testing;

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;

use futures_util::stream::{self, BoxStream, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::UnixStream;

use crate::error::CompositorError;
use crate::event::Event;
use crate::model::{
    LayoutTarget, Logical, Mode, Output, Snapshot, Window, WindowId, WindowTarget, Workspace,
    WorkspaceId, WorkspaceTarget, is_built_in,
};
use event::{EventState, WireLayouts};

pub(crate) const CAPABILITIES: crate::Capabilities = crate::Capabilities {
    floating: false,
    workspace_reorder: true,
};

/// Niri answers one request per connection and then closes it, so there is no connection to hold —
/// only the path to open the next one at. `EventStream` is the single exception, and it owns its
/// own connection for as long as the stream lives.
#[derive(Debug, Clone)]
pub struct Niri {
    socket: PathBuf,
}

impl Niri {
    pub fn at(socket: impl Into<PathBuf>) -> Self {
        Self {
            socket: socket.into(),
        }
    }

    pub(crate) async fn snapshot(&self) -> Result<Snapshot, CompositorError> {
        let (outputs, workspaces, windows, keyboard, focused_output) = tokio::try_join!(
            self.fetch::<BTreeMap<String, WireOutput>>("Outputs"),
            self.fetch::<Vec<Workspace>>("Workspaces"),
            self.fetch::<Vec<Window>>("Windows"),
            self.fetch::<WireLayouts>("KeyboardLayouts"),
            self.fetch::<Option<WireFocusedOutput>>("FocusedOutput"),
        )?;

        Ok(Snapshot {
            outputs: outputs.into_values().map(WireOutput::into_model).collect(),
            focused_window: windows
                .iter()
                .find(|window| window.is_focused)
                .map(|window| window.id),
            workspaces,
            windows,
            keyboard: keyboard.into(),
            focused_output: focused_output.map(|output| output.name),
        })
    }

    pub(crate) async fn events(&self) -> Result<BoxStream<'static, Event>, CompositorError> {
        let mut lines = self.open(&json!("EventStream")).await?;
        // Niri acknowledges the subscription before it sends anything, and a refusal here has to
        // surface as an error rather than as a stream that silently yields nothing.
        let reply = next_line(&mut lines).await?;
        unwrap_reply(&reply)?;

        let start = (lines, EventState::default(), VecDeque::new());
        Ok(
            stream::unfold(start, |(mut lines, mut state, mut pending)| async move {
                loop {
                    if let Some(event) = pending.pop_front() {
                        return Some((event, (lines, state, pending)));
                    }
                    let line = lines.next_line().await.ok()??;
                    pending.extend(state.decode(&line));
                }
            })
            .boxed(),
        )
    }

    pub(crate) async fn switch_keyboard_layout(
        &self,
        to: LayoutTarget,
    ) -> Result<(), CompositorError> {
        self.act(&layout_action(to)).await
    }

    pub(crate) async fn focus_workspace(&self, to: WorkspaceTarget) -> Result<(), CompositorError> {
        self.act(&workspace_action(to)).await
    }

    pub(crate) async fn rename_workspace(
        &self,
        id: WorkspaceId,
        name: Option<&str>,
    ) -> Result<(), CompositorError> {
        self.act(&rename_action(id, name)).await
    }

    pub(crate) async fn focus_window(&self, to: WindowTarget) -> Result<(), CompositorError> {
        self.act(&window_action(to)).await
    }

    pub(crate) async fn move_workspace_to_output(
        &self,
        id: WorkspaceId,
        connector: &str,
    ) -> Result<(), CompositorError> {
        self.act(&move_to_output_action(id, connector)).await
    }

    pub(crate) async fn reorder_workspace(
        &self,
        id: WorkspaceId,
        index: u8,
    ) -> Result<(), CompositorError> {
        self.act(&reorder_action(id, index)).await
    }

    pub(crate) async fn move_window_to_workspace(
        &self,
        window: WindowId,
        to: &WorkspaceTarget,
    ) -> Result<(), CompositorError> {
        self.act(&move_window_action(window, to)?).await
    }

    pub(crate) async fn close_window(&self, id: WindowId) -> Result<(), CompositorError> {
        self.act(&close_action(id)).await
    }

    pub(crate) async fn focus_output(&self, connector: &str) -> Result<(), CompositorError> {
        self.act(&focus_output_action(connector)).await
    }

    pub(crate) async fn set_output_enabled(
        &self,
        connector: &str,
        on: bool,
    ) -> Result<(), CompositorError> {
        self.request(&output_request(connector, on)).await.map(drop)
    }

    async fn fetch<T: for<'de> Deserialize<'de>>(
        &self,
        name: &'static str,
    ) -> Result<T, CompositorError> {
        let reply = self.request(&json!(name)).await?;
        let payload = reply
            .get(name)
            .ok_or_else(|| CompositorError::Protocol(format!("reply carried no `{name}`")))?;
        T::deserialize(payload).map_err(CompositorError::protocol)
    }

    async fn act(&self, action: &Value) -> Result<(), CompositorError> {
        self.request(&json!({ "Action": action })).await.map(drop)
    }

    async fn request(&self, request: &Value) -> Result<Value, CompositorError> {
        let mut lines = self.open(request).await?;
        let reply = next_line(&mut lines).await?;
        unwrap_reply(&reply)
    }

    async fn open(&self, request: &Value) -> Result<Lines<BufReader<UnixStream>>, CompositorError> {
        let mut stream = UnixStream::connect(&self.socket)
            .await
            .map_err(|error| CompositorError::connect(&self.socket, error))?;

        let mut line = serde_json::to_vec(request).map_err(CompositorError::protocol)?;
        line.push(b'\n');
        stream
            .write_all(&line)
            .await
            .map_err(|error| CompositorError::connect(&self.socket, error))?;
        stream.flush().await.map_err(CompositorError::protocol)?;

        Ok(BufReader::new(stream).lines())
    }
}

pub(crate) fn from_env() -> Option<Niri> {
    let socket = std::env::var_os("NIRI_SOCKET")?;
    match socket.is_empty() {
        true => None,
        false => Some(Niri::at(PathBuf::from(socket))),
    }
}

async fn next_line(lines: &mut Lines<BufReader<UnixStream>>) -> Result<String, CompositorError> {
    lines
        .next_line()
        .await
        .map_err(CompositorError::protocol)?
        .ok_or(CompositorError::Closed)
}

fn unwrap_reply(line: &str) -> Result<Value, CompositorError> {
    match serde_json::from_str::<Reply>(line) {
        Ok(Reply::Ok(payload)) => Ok(payload),
        Ok(Reply::Err(reason)) => Err(CompositorError::Refused(reason)),
        Err(error) => Err(CompositorError::protocol(error)),
    }
}

#[derive(Deserialize)]
enum Reply {
    Ok(Value),
    Err(String),
}

#[derive(Deserialize)]
struct WireFocusedOutput {
    name: String,
}

#[derive(Deserialize)]
struct WireMode {
    width: u32,
    height: u32,
    refresh_rate: u32,
}

#[derive(Deserialize)]
struct WireOutput {
    name: String,
    #[serde(default)]
    make: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    modes: Vec<WireMode>,
    /// An index into `modes`, and `null` for an output that is switched off — niri has no
    /// `enabled` field, so this is the only thing that says so.
    #[serde(default)]
    current_mode: Option<usize>,
    #[serde(default)]
    logical: Option<Logical>,
}

impl WireOutput {
    fn into_model(self) -> Output {
        let current_mode = self
            .current_mode
            .and_then(|index| self.modes.get(index))
            .map(|mode| Mode {
                width: mode.width,
                height: mode.height,
                refresh_mhz: mode.refresh_rate,
            });

        Output {
            built_in: is_built_in(&self.name),
            connector: self.name,
            make: self.make,
            model: self.model,
            description: None,
            logical: self.logical,
            enabled: current_mode.is_some(),
            current_mode,
        }
    }
}

fn layout_action(to: LayoutTarget) -> Value {
    let layout = match to {
        LayoutTarget::Next => json!("Next"),
        LayoutTarget::Prev => json!("Prev"),
        LayoutTarget::Index(index) => json!({ "Index": index }),
    };
    json!({ "SwitchLayout": { "layout": layout } })
}

fn workspace_action(to: WorkspaceTarget) -> Value {
    // `Next`/`Prev` are separate actions, not a reference: niri's directional focus follows the
    // monitor layout, which an index cannot express.
    let reference = match to {
        WorkspaceTarget::Next => return json!({ "FocusWorkspaceDown": {} }),
        WorkspaceTarget::Prev => return json!({ "FocusWorkspaceUp": {} }),
        WorkspaceTarget::Id(id) => json!({ "Id": id.0 }),
        WorkspaceTarget::Index(index) => json!({ "Index": index }),
        WorkspaceTarget::Name(name) => json!({ "Name": name }),
    };
    json!({ "FocusWorkspace": { "reference": reference } })
}

fn window_action(to: WindowTarget) -> Value {
    match to {
        WindowTarget::Next => json!({ "FocusWindowDownOrColumnRight": {} }),
        WindowTarget::Prev => json!({ "FocusWindowUpOrColumnLeft": {} }),
        WindowTarget::Id(id) => json!({ "FocusWindow": { "id": id.0 } }),
    }
}

fn rename_action(id: WorkspaceId, name: Option<&str>) -> Value {
    let reference = json!({ "Id": id.0 });
    match name {
        Some(name) => json!({ "SetWorkspaceName": { "name": name, "workspace": reference } }),
        None => json!({ "UnsetWorkspaceName": { "reference": reference } }),
    }
}

fn workspace_reference(to: &WorkspaceTarget) -> Result<Value, CompositorError> {
    match to {
        WorkspaceTarget::Id(id) => Ok(json!({ "Id": id.0 })),
        WorkspaceTarget::Index(index) => Ok(json!({ "Index": index })),
        WorkspaceTarget::Name(name) => Ok(json!({ "Name": name })),
        WorkspaceTarget::Next | WorkspaceTarget::Prev => Err(CompositorError::Unavailable(
            "move a window to a workspace named only as next or previous",
        )),
    }
}

fn move_to_output_action(id: WorkspaceId, connector: &str) -> Value {
    json!({ "MoveWorkspaceToMonitor": { "output": connector, "reference": { "Id": id.0 } } })
}

fn reorder_action(id: WorkspaceId, index: u8) -> Value {
    json!({ "MoveWorkspaceToIndex": { "index": index, "reference": { "Id": id.0 } } })
}

fn move_window_action(window: WindowId, to: &WorkspaceTarget) -> Result<Value, CompositorError> {
    let reference = workspace_reference(to)?;
    Ok(json!({
        "MoveWindowToWorkspace": { "reference": reference, "window_id": window.0, "focus": false }
    }))
}

fn close_action(id: WindowId) -> Value {
    json!({ "CloseWindow": { "id": id.0 } })
}

fn focus_output_action(connector: &str) -> Value {
    json!({ "FocusMonitor": { "output": connector } })
}

fn output_request(connector: &str, on: bool) -> Value {
    let action = match on {
        true => "On",
        false => "Off",
    };
    json!({ "Output": { "output": connector, "action": action } })
}

#[cfg(test)]
mod tests {
    use futures_util::StreamExt;
    use serde_json::json;

    use super::testing::FakeNiri;
    use super::*;
    use crate::event::Resync;
    use crate::model::WindowId;

    /// Captured from a live niri 26.04 and re-wrapped: `niri msg -j` prints the payload without the
    /// `{"Ok": …}` envelope the socket actually carries.
    fn reply(name: &str, payload: Value) -> Vec<String> {
        vec![json!({ "Ok": { name: payload } }).to_string()]
    }

    fn outputs() -> Value {
        json!({
            "eDP-1": {
                "name": "eDP-1", "make": "Samsung Display Corp.", "model": "ATNA60CL10-0",
                "modes": [{ "width": 2880, "height": 1800, "refresh_rate": 120000 }],
                "current_mode": 0,
                "logical": { "x": 0, "y": 0, "width": 2304, "height": 1440,
                             "scale": 1.25, "transform": "Normal" }
            },
            "DP-3": {
                "name": "DP-3", "make": "Dell", "model": "U2720Q",
                "modes": [{ "width": 3840, "height": 2160, "refresh_rate": 60000 }],
                "current_mode": null, "logical": null
            }
        })
    }

    fn snapshot_server() -> FakeNiri {
        FakeNiri::spawn(|request| match request {
            "\"Outputs\"" => reply("Outputs", outputs()),
            "\"Workspaces\"" => reply(
                "Workspaces",
                json!([{ "id": 5, "idx": 1, "name": null, "output": "eDP-1",
                         "is_urgent": false, "is_active": true, "is_focused": true,
                         "active_window_id": 9 }]),
            ),
            "\"Windows\"" => reply(
                "Windows",
                json!([{ "id": 9, "title": "a terminal", "app_id": "foot", "pid": 4265,
                         "workspace_id": 5, "is_focused": true, "is_floating": false,
                         "is_urgent": false,
                         "layout": { "pos_in_scrolling_layout": [2, 0] } }]),
            ),
            "\"KeyboardLayouts\"" => reply(
                "KeyboardLayouts",
                json!({ "names": ["Polish", "Russian"], "current_idx": 1 }),
            ),
            "\"FocusedOutput\"" => reply("FocusedOutput", json!({ "name": "eDP-1" })),
            other => panic!("the fake was asked for {other}"),
        })
    }

    #[tokio::test]
    async fn a_snapshot_reads_every_part_of_the_model() {
        let server = snapshot_server();
        let snapshot = Niri::at(&server.socket).snapshot().await.expect("snapshot");

        assert_eq!(snapshot.focused_output.as_deref(), Some("eDP-1"));
        assert_eq!(snapshot.focused_window, Some(WindowId(9)));
        assert_eq!(snapshot.workspaces[0].id, WorkspaceId(5));
        assert_eq!(snapshot.workspaces[0].idx, Some(1));
        assert_eq!(snapshot.windows[0].app_id.as_deref(), Some("foot"));
        assert_eq!(snapshot.windows[0].layout_order, Some(2));
        assert_eq!(snapshot.keyboard.names, ["Polish", "Russian"]);
        assert_eq!(snapshot.keyboard.codes, ["PL", "RU"]);
        assert_eq!(snapshot.keyboard.current, Some(1));
    }

    /// Niri has no `enabled` field, so a null `current_mode` is the only thing that says an output
    /// is switched off. Reading it as "no mode known" would leave a dead monitor in the list.
    #[tokio::test]
    async fn an_output_with_no_current_mode_is_disabled() {
        let server = snapshot_server();
        let snapshot = Niri::at(&server.socket).snapshot().await.expect("snapshot");

        let built_in = snapshot
            .outputs
            .iter()
            .find(|output| output.connector == "eDP-1")
            .expect("eDP-1");
        let external = snapshot
            .outputs
            .iter()
            .find(|output| output.connector == "DP-3")
            .expect("DP-3");

        assert!(built_in.enabled);
        assert!(built_in.built_in);
        assert_eq!(
            built_in.current_mode,
            Some(Mode {
                width: 2880,
                height: 1800,
                refresh_mhz: 120_000
            })
        );
        assert!(!external.enabled);
        assert!(!external.built_in);
        assert_eq!(external.current_mode, None);
    }

    #[tokio::test]
    async fn a_refusal_carries_the_reason_niri_gave() {
        let server = FakeNiri::spawn(|_| vec![json!({ "Err": "no such workspace" }).to_string()]);

        let failure = Niri::at(&server.socket)
            .focus_workspace(WorkspaceTarget::Index(9))
            .await
            .expect_err("refused");

        assert!(matches!(failure, CompositorError::Refused(reason) if reason.contains("no such")));
    }

    #[tokio::test]
    async fn a_connection_closed_before_a_reply_is_not_a_protocol_error() {
        let server = FakeNiri::silent();

        let failure = Niri::at(&server.socket)
            .focus_window(WindowTarget::Next)
            .await
            .expect_err("closed");

        assert!(matches!(failure, CompositorError::Closed));
    }

    #[tokio::test]
    async fn an_undecodable_reply_is_a_protocol_error() {
        let server = FakeNiri::spawn(|_| vec!["not json at all".to_owned()]);

        let failure = Niri::at(&server.socket)
            .focus_window(WindowTarget::Next)
            .await
            .expect_err("garbled");

        assert!(matches!(failure, CompositorError::Protocol(_)));
    }

    #[tokio::test]
    async fn a_missing_socket_names_the_path_it_tried() {
        let failure = Niri::at("/nonexistent/niri.sock")
            .snapshot()
            .await
            .expect_err("no socket");

        assert!(
            failure.to_string().contains("/nonexistent/niri.sock"),
            "expected the path in {failure}"
        );
    }

    /// The one test that catches an upstream rename: every target, against the literal JSON niri
    /// 26.04 accepts.
    #[test]
    fn every_addon_names_its_subject_rather_than_acting_on_the_focused_one() {
        assert_eq!(
            move_to_output_action(WorkspaceId(4), "DP-3"),
            json!({ "MoveWorkspaceToMonitor": { "output": "DP-3", "reference": { "Id": 4 } } })
        );
        assert_eq!(
            reorder_action(WorkspaceId(4), 2),
            json!({ "MoveWorkspaceToIndex": { "index": 2, "reference": { "Id": 4 } } })
        );
        assert_eq!(
            close_action(WindowId(9)),
            json!({ "CloseWindow": { "id": 9 } })
        );
        assert_eq!(
            focus_output_action("DP-3"),
            json!({ "FocusMonitor": { "output": "DP-3" } })
        );
    }

    #[test]
    fn moving_a_window_to_a_workspace_does_not_follow_it_there() {
        let action = move_window_action(WindowId(9), &WorkspaceTarget::Id(WorkspaceId(4)))
            .expect("an id is a reference");
        assert_eq!(
            action,
            json!({
                "MoveWindowToWorkspace": {
                    "reference": { "Id": 4 },
                    "window_id": 9,
                    "focus": false
                }
            }),
            "niri defaults focus to true, which would drag the user after the window"
        );
    }

    #[test]
    fn a_relative_workspace_is_not_a_reference_a_window_can_be_moved_to() {
        assert!(matches!(
            move_window_action(WindowId(9), &WorkspaceTarget::Next),
            Err(CompositorError::Unavailable(_))
        ));
    }

    #[test]
    fn every_target_serializes_to_the_action_niri_expects() {
        assert_eq!(
            layout_action(LayoutTarget::Index(2)),
            json!({ "SwitchLayout": { "layout": { "Index": 2 } } })
        );
        assert_eq!(
            layout_action(LayoutTarget::Next),
            json!({ "SwitchLayout": { "layout": "Next" } })
        );
        assert_eq!(
            layout_action(LayoutTarget::Prev),
            json!({ "SwitchLayout": { "layout": "Prev" } })
        );

        assert_eq!(
            workspace_action(WorkspaceTarget::Id(WorkspaceId(5))),
            json!({ "FocusWorkspace": { "reference": { "Id": 5 } } })
        );
        assert_eq!(
            workspace_action(WorkspaceTarget::Index(3)),
            json!({ "FocusWorkspace": { "reference": { "Index": 3 } } })
        );
        assert_eq!(
            workspace_action(WorkspaceTarget::Name("chat".to_owned())),
            json!({ "FocusWorkspace": { "reference": { "Name": "chat" } } })
        );
        assert_eq!(
            workspace_action(WorkspaceTarget::Next),
            json!({ "FocusWorkspaceDown": {} })
        );
        assert_eq!(
            workspace_action(WorkspaceTarget::Prev),
            json!({ "FocusWorkspaceUp": {} })
        );

        assert_eq!(
            window_action(WindowTarget::Id(WindowId(9))),
            json!({ "FocusWindow": { "id": 9 } })
        );
        assert_eq!(
            window_action(WindowTarget::Next),
            json!({ "FocusWindowDownOrColumnRight": {} })
        );
        assert_eq!(
            window_action(WindowTarget::Prev),
            json!({ "FocusWindowUpOrColumnLeft": {} })
        );

        assert_eq!(
            rename_action(WorkspaceId(3), Some("chat")),
            json!({ "SetWorkspaceName": { "name": "chat", "workspace": { "Id": 3 } } })
        );
        assert_eq!(
            rename_action(WorkspaceId(3), None),
            json!({ "UnsetWorkspaceName": { "reference": { "Id": 3 } } })
        );

        assert_eq!(
            output_request("eDP-1", true),
            json!({ "Output": { "output": "eDP-1", "action": "On" } })
        );
        assert_eq!(
            output_request("eDP-1", false),
            json!({ "Output": { "output": "eDP-1", "action": "Off" } })
        );
    }

    async fn events_from(lines: Vec<Value>) -> Vec<Event> {
        let scripted: Vec<String> = std::iter::once(json!({ "Ok": { "Handled": null } }))
            .chain(lines)
            .map(|line| line.to_string())
            .collect();
        let server = FakeNiri::spawn(move |_| scripted.clone());

        Niri::at(&server.socket)
            .events()
            .await
            .expect("subscribed")
            .collect()
            .await
    }

    #[tokio::test]
    async fn the_stream_consumes_the_acknowledgement_and_decodes_what_follows() {
        let events = events_from(vec![
            json!({ "WindowClosed": { "id": 9 } }),
            json!({ "WindowFocusChanged": { "id": null } }),
        ])
        .await;

        assert_eq!(
            events,
            [
                Event::WindowClosed(WindowId(9)),
                Event::WindowFocusChanged(None),
            ]
        );
    }

    /// A variant added by a future niri must not end the subscription.
    #[tokio::test]
    async fn an_unrecognized_event_is_skipped_and_the_stream_survives() {
        let events = events_from(vec![
            json!({ "SomethingNiriAddedLater": { "whatever": true } }),
            json!({ "WindowClosed": { "id": 4 } }),
        ])
        .await;

        assert_eq!(events, [Event::WindowClosed(WindowId(4))]);
    }

    #[tokio::test]
    async fn a_layout_switch_is_named_from_the_layouts_last_announced() {
        let events = events_from(vec![
            json!({ "KeyboardLayoutsChanged": {
                "keyboard_layouts": { "names": ["Polish", "Russian"], "current_idx": 0 } } }),
            json!({ "KeyboardLayoutSwitched": { "idx": 1 } }),
        ])
        .await;

        assert_eq!(
            events[1],
            Event::KeyboardLayoutSwitched {
                idx: 1,
                name: Some("Russian".to_owned()),
            }
        );
    }

    /// Niri emits no output event. A monitor leaving shows up as its workspaces being reassigned,
    /// and without noticing that shift the daemon keeps a dead monitor in its list forever.
    #[tokio::test]
    async fn losing_a_monitor_asks_for_an_output_resync() {
        let workspaces = |outputs: [&str; 2]| {
            json!({ "WorkspacesChanged": { "workspaces": [
                { "id": 1, "output": outputs[0] },
                { "id": 2, "output": outputs[1] },
            ] } })
        };

        let events = events_from(vec![
            workspaces(["eDP-1", "DP-3"]),
            workspaces(["eDP-1", "DP-3"]),
            workspaces(["eDP-1", "eDP-1"]),
        ])
        .await;

        assert_eq!(events[0], Event::Resync(Resync::Outputs));
        assert!(
            matches!(events[1], Event::WorkspacesChanged(_)),
            "the first list is also delivered, got {:?}",
            events[1]
        );
        assert!(
            matches!(events[2], Event::WorkspacesChanged(_)),
            "an unchanged monitor set must not resync, got {:?}",
            events[2]
        );
        assert_eq!(events[3], Event::Resync(Resync::Outputs));
    }

    /// Three events `_old` never saw, each feeding a model field that would otherwise only ever
    /// update on a full refresh.
    #[tokio::test]
    async fn urgency_and_layout_order_arrive_incrementally() {
        let events = events_from(vec![
            json!({ "WindowUrgencyChanged": { "id": 9, "urgent": true } }),
            json!({ "WorkspaceUrgencyChanged": { "id": 5, "urgent": false } }),
            json!({ "WindowLayoutsChanged": { "changes": [
                [9, { "pos_in_scrolling_layout": [3, 1] }],
                [4, { "pos_in_scrolling_layout": null }],
            ] } }),
        ])
        .await;

        assert_eq!(
            events,
            [
                Event::WindowUrgencyChanged {
                    id: WindowId(9),
                    urgent: true
                },
                Event::WorkspaceUrgencyChanged {
                    id: WorkspaceId(5),
                    urgent: false
                },
                Event::WindowLayoutsChanged(vec![(WindowId(9), Some(3)), (WindowId(4), None)]),
            ]
        );
    }

    /// Niri reloads its own configuration in place, and `kb_layout` is the part of it this crate
    /// caches — without the resync the panel shows a layout list the compositor has discarded.
    #[tokio::test]
    async fn a_successful_config_reload_asks_for_a_keyboard_resync() {
        let events = events_from(vec![
            json!({ "ConfigLoaded": { "failed": true } }),
            json!({ "ConfigLoaded": { "failed": false } }),
        ])
        .await;

        assert_eq!(events, [Event::Resync(Resync::Keyboard)]);
    }

    #[tokio::test]
    async fn a_hostile_window_title_is_capped_before_it_leaves_the_crate() {
        let long = "x".repeat(600);
        let title = format!("\u{2068}spoof\u{2069}{long}");
        let server = FakeNiri::spawn(move |request| match request {
            "\"Windows\"" => reply(
                "Windows",
                json!([{ "id": 1, "title": title, "app_id": "a", "is_focused": false }]),
            ),
            "\"Outputs\"" => reply("Outputs", json!({})),
            "\"Workspaces\"" => reply("Workspaces", json!([])),
            "\"KeyboardLayouts\"" => {
                reply("KeyboardLayouts", json!({ "names": [], "current_idx": 0 }))
            }
            "\"FocusedOutput\"" => reply("FocusedOutput", json!(null)),
            other => panic!("the fake was asked for {other}"),
        });

        let snapshot = Niri::at(&server.socket).snapshot().await.expect("snapshot");
        let title = snapshot.windows[0].title.clone().expect("a title");

        assert!(!title.contains('\u{2068}'), "bidi overrides survived");
        assert_eq!(title.chars().count(), 512);
        assert!(title.starts_with("spoof"));
    }
}
