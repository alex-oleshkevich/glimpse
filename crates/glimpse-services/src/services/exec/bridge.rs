use serde::Serialize;
use serde_json::Value;

use super::wire::{CompositorCall, MprisAction, MprisCall, Step, WindowTarget, WorkspaceTarget};
use crate::CommandError;
use crate::services::compositor::{CompositorHandle, CompositorState, WindowRef, WorkspaceRef};
use crate::services::mpris::{MprisHandle, MprisPlayers, Playback, PlayerAction};

const TEXT_CAP: usize = 256;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Player {
    id: String,
    identity: String,
    desktop_entry: Option<String>,
    playback: &'static str,
    current: bool,
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    length_us: Option<i64>,
    position_us: i64,
    position_at: i64,
    rate: f64,
    can: Can,
}

#[derive(Serialize)]
struct Can {
    play: bool,
    pause: bool,
    previous: bool,
    next: bool,
    seek: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Compositor {
    name: Option<String>,
    workspaces: Vec<Workspace>,
    windows: Vec<Window>,
    outputs: Vec<Output>,
}

#[derive(Serialize)]
struct Workspace {
    id: u64,
    index: Option<u8>,
    name: Option<String>,
    output: Option<String>,
    active: bool,
    focused: bool,
    urgent: bool,
    windows: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Window {
    id: u64,
    title: Option<String>,
    app_id: Option<String>,
    workspace: Option<u64>,
    focused: bool,
    urgent: bool,
}

#[derive(Serialize)]
struct Output {
    connector: String,
    label: Option<String>,
    focused: bool,
}

fn text(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(|value| glimpse_utils::clean(value, TEXT_CAP))
}

pub fn mpris_value(players: &MprisPlayers) -> Value {
    let players: Vec<Player> = players
        .players
        .iter()
        .map(|player| Player {
            id: glimpse_utils::clean(&player.id, TEXT_CAP),
            identity: glimpse_utils::clean(&player.identity, TEXT_CAP),
            desktop_entry: text(&player.desktop_entry),
            playback: match player.playback {
                Playback::Playing => "playing",
                Playback::Paused => "paused",
                Playback::Stopped | Playback::Unknown => "stopped",
            },
            current: player.current,
            title: text(&player.title),
            artist: text(&player.artist),
            album: text(&player.album),
            length_us: player.length_us,
            position_us: player.position_us,
            position_at: player.position_at.timestamp_millis(),
            rate: player.rate,
            can: Can {
                play: player.can.play,
                pause: player.can.pause,
                previous: player.can.previous,
                next: player.can.next,
                seek: player.can.seek,
            },
        })
        .collect();
    serde_json::json!({ "players": players })
}

pub fn compositor_value(state: &CompositorState) -> Value {
    let value = Compositor {
        name: state.status.as_ref().map(|status| status.name.clone()),
        workspaces: state
            .workspaces
            .iter()
            .flat_map(|workspaces| &workspaces.workspaces)
            .map(|workspace| Workspace {
                id: workspace.id,
                index: workspace.index,
                name: text(&workspace.name),
                output: workspace.output.clone(),
                active: workspace.active,
                focused: workspace.focused,
                urgent: workspace.urgent,
                windows: workspace.windows,
            })
            .collect(),
        windows: state
            .windows
            .iter()
            .flat_map(|windows| &windows.windows)
            .map(|window| Window {
                id: window.id,
                title: text(&window.title),
                app_id: text(&window.app_id),
                workspace: window.workspace,
                focused: window.focused,
                urgent: window.urgent,
            })
            .collect(),
        outputs: state
            .outputs
            .iter()
            .flat_map(|outputs| &outputs.outputs)
            .map(|output| Output {
                connector: output.connector.clone(),
                label: text(&output.label),
                focused: output.focused,
            })
            .collect(),
    };
    serde_json::to_value(value).unwrap_or(Value::Null)
}

pub async fn mpris_call(
    mpris: MprisHandle,
    player: String,
    call: MprisCall,
) -> Result<(), CommandError> {
    match call {
        MprisCall::Control { action } => {
            let action = match action {
                MprisAction::Play => PlayerAction::Play,
                MprisAction::Pause => PlayerAction::Pause,
                MprisAction::PlayPause => PlayerAction::PlayPause,
                MprisAction::Stop => PlayerAction::Stop,
                MprisAction::Previous => PlayerAction::Previous,
                MprisAction::Next => PlayerAction::Next,
                MprisAction::Raise => PlayerAction::Raise,
            };
            mpris.control(player, action).await
        }
        MprisCall::Seek { offset } => mpris.seek(player, offset).await,
        MprisCall::SetPosition { position } => mpris.set_position(player, position).await,
    }
}

fn workspace(target: WorkspaceTarget) -> WorkspaceRef {
    match target {
        WorkspaceTarget::Id { id } => WorkspaceRef::Id { id },
        WorkspaceTarget::Index { index } => WorkspaceRef::Index { index },
        WorkspaceTarget::Name { name } => WorkspaceRef::Name { name },
        WorkspaceTarget::Step(Step::Next) => WorkspaceRef::Next,
        WorkspaceTarget::Step(Step::Prev) => WorkspaceRef::Prev,
    }
}

pub async fn compositor_call(
    compositor: CompositorHandle,
    call: CompositorCall,
) -> Result<(), CommandError> {
    match call {
        CompositorCall::FocusWorkspace { target } => {
            compositor.focus_workspace(workspace(target)).await
        }
        CompositorCall::FocusWindow { target } => {
            let target = match target {
                WindowTarget::Id { id } => WindowRef::Id { id },
                WindowTarget::Step(Step::Next) => WindowRef::Next,
                WindowTarget::Step(Step::Prev) => WindowRef::Prev,
            };
            compositor.focus_window(target).await
        }
        CompositorCall::MoveWindow { window, target } => {
            compositor
                .move_window_to_workspace(window, workspace(target))
                .await
        }
        CompositorCall::RenameWorkspace { id, name } => {
            let name = name.map(|name| glimpse_utils::clean(&name, TEXT_CAP));
            compositor.rename_workspace(id, name).await
        }
        CompositorCall::CloseWindow { id } => compositor.close_window(id).await,
    }
}

#[cfg(test)]
mod tests;
