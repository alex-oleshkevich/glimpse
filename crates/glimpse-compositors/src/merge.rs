use crate::event::{Event, Resync};
use crate::model::Snapshot;

pub fn apply(state: &mut Snapshot, event: Event) -> Option<Resync> {
    match event {
        Event::WorkspacesChanged {
            workspaces,
            focused,
        } => {
            state.focus.output = focused
                .and_then(|id| workspaces.iter().find(|workspace| workspace.id == id))
                .and_then(|workspace| workspace.output.clone());
            state.workspaces = workspaces;
        }
        Event::WorkspaceActivated { id, focused } => {
            let Some(activated) = state.workspaces.iter().find(|workspace| workspace.id == id)
            else {
                return Some(Resync::Structure);
            };
            let output = activated.output.clone();
            for workspace in &mut state.workspaces {
                if workspace.output == output {
                    workspace.active = workspace.id == id;
                }
            }
            if focused {
                state.focus.output = output;
            }
        }
        Event::WindowsChanged { windows, focused } => {
            state.windows = windows;
            state.focus.window = focused;
        }
        Event::WindowOpenedOrChanged { window, focused } => {
            let id = window.id;
            match state.windows.iter_mut().find(|it| it.id == id) {
                Some(found) => *found = window,
                None => state.windows.push(window),
            }
            if focused {
                state.focus.window = Some(id);
            }
        }
        Event::WindowClosed(id) => state.windows.retain(|window| window.id != id),
        Event::WindowFocusChanged(id) => {
            state.focus.window = id;
            if let Some(found) = state.windows.iter_mut().find(|it| Some(it.id) == id) {
                found.urgent = false;
            }
        }
        Event::WindowUrgencyChanged { id, urgent } => {
            if let Some(found) = state.windows.iter_mut().find(|it| it.id == id) {
                found.urgent = urgent;
            }
        }
        Event::WindowLayoutsChanged(orders) => {
            for (id, order) in orders {
                if let Some(found) = state.windows.iter_mut().find(|it| it.id == id) {
                    found.order = order;
                }
            }
        }
        Event::KeyboardLayoutsChanged(layouts) => state.keyboard = layouts,
        Event::KeyboardLayoutSwitched(index) => {
            let count = state.keyboard.names.len().max(state.keyboard.codes.len());
            state.keyboard.current =
                (usize::try_from(index).is_ok_and(|at| at < count)).then_some(index);
        }
        Event::CastsChanged(casts) => state.casts = casts,
        Event::CastStartedOrChanged(cast) => {
            match state
                .casts
                .iter_mut()
                .find(|existing| existing.id == cast.id)
            {
                Some(existing) => *existing = cast,
                None => state.casts.push(cast),
            }
        }
        Event::CastStopped(id) => state.casts.retain(|cast| cast.id != id),
        Event::Resync(resync) => return Some(resync),
    }
    None
}

#[cfg(test)]
mod tests;
