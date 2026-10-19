use std::collections::VecDeque;
use std::future::Future;

use futures_util::stream::{self, BoxStream, StreamExt};
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

use crate::model::{Cast, KeyboardLayouts, Window, WindowId, Workspace, WorkspaceId};

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    WorkspacesChanged(Vec<Workspace>),
    WorkspaceActivated {
        id: WorkspaceId,
        focused: bool,
    },
    WorkspaceActiveWindowChanged {
        workspace: WorkspaceId,
        window: Option<WindowId>,
    },
    WorkspaceUrgencyChanged {
        id: WorkspaceId,
        urgent: bool,
    },
    WindowsChanged(Vec<Window>),
    WindowOpenedOrChanged(Window),
    WindowClosed(WindowId),
    WindowFocusChanged(Option<WindowId>),
    WindowUrgencyChanged {
        id: WindowId,
        urgent: bool,
    },
    WindowLayoutsChanged(Vec<(WindowId, Option<u16>)>),
    KeyboardLayoutsChanged(KeyboardLayouts),
    KeyboardLayoutSwitched {
        idx: usize,
        name: Option<String>,
    },
    CastsChanged(Vec<Cast>),
    CastStartedOrChanged {
        cast: Cast,
    },
    CastStopped(u64),
    /// The compositor said something changed without saying what. The caller re-fetches the named
    /// part of the snapshot. Hyprland produces most of these; niri produces a handful.
    Resync(Resync),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resync {
    Structure,
    Keyboard,
    Outputs,
}

pub(crate) trait LineDecoder: Send + 'static {
    fn decode(&mut self, line: &str) -> impl Future<Output = Vec<Event>> + Send;
}

pub(crate) fn line_events<R, D>(reader: R, decoder: D) -> BoxStream<'static, Event>
where
    R: AsyncBufRead + Unpin + Send + 'static,
    D: LineDecoder,
{
    let start = (reader, decoder, VecDeque::new());
    stream::unfold(start, |(mut reader, mut decoder, mut pending)| async move {
        loop {
            if let Some(event) = pending.pop_front() {
                return Some((event, (reader, decoder, pending)));
            }
            let mut bytes = Vec::new();
            match reader.read_until(b'\n', &mut bytes).await {
                Ok(0) | Err(_) => return None,
                Ok(_) => {}
            }
            let line = String::from_utf8_lossy(&bytes);
            let line = line.trim_end_matches(['\n', '\r']);
            pending.extend(decoder.decode(line).await);
        }
    })
    .boxed()
}

#[cfg(test)]
mod tests;
