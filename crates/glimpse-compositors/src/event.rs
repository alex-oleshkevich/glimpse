use std::collections::VecDeque;
use std::future::Future;

use futures_util::stream::{self, BoxStream, StreamExt};
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

use crate::model::{Cast, KeyboardLayouts, Window, WindowId, Workspace, WorkspaceId};

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    WorkspacesChanged {
        workspaces: Vec<Workspace>,
        focused: Option<WorkspaceId>,
    },
    WorkspaceActivated {
        id: WorkspaceId,
        focused: bool,
    },
    WindowsChanged {
        windows: Vec<Window>,
        focused: Option<WindowId>,
    },
    WindowOpenedOrChanged {
        window: Window,
        focused: bool,
    },
    WindowClosed(WindowId),
    WindowFocusChanged(Option<WindowId>),
    WindowUrgencyChanged {
        id: WindowId,
        urgent: bool,
    },
    WindowLayoutsChanged(Vec<(WindowId, Option<u16>)>),
    KeyboardLayoutsChanged(KeyboardLayouts),
    KeyboardLayoutSwitched(u32),
    CastsChanged(Vec<Cast>),
    CastStartedOrChanged(Cast),
    CastStopped(u64),
    Resync(Resync),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resync {
    Structure,
    Keyboard,
    Outputs,
}

pub trait LineDecoder: Send + 'static {
    type Event: Send + 'static;

    fn decode(&mut self, line: &str) -> impl Future<Output = Vec<Self::Event>> + Send;
}

pub fn line_events<R, D>(reader: R, decoder: D) -> BoxStream<'static, D::Event>
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
