//! Compositor state and control for niri and Hyprland, behind one model.
//!
//! Both compositors are reached over their own Unix socket — this crate touches no `wl_` object,
//! so the `glimpsed/src/wayland/` rule does not apply to it.

mod error;
mod event;
mod hyprland;
mod keyboard;
mod model;
mod niri;

pub use error::CompositorError;
pub use event::{Event, Resync};
pub use hyprland::Hyprland;
pub use keyboard::layout_code;
pub use model::{
    KeyboardLayouts, LayoutTarget, Logical, Mode, Output, Snapshot, Window, WindowId, WindowTarget,
    Workspace, WorkspaceId, WorkspaceTarget,
};
pub use niri::Niri;

use futures_util::stream::BoxStream;

/// What a caller can do that depends on which compositor is running. One field, because `floating`
/// is the only thing niri and Hyprland disagree on that anything acts upon — the rest of what a
/// compositor supports is answered by whether it is `Unsupported`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub floating: bool,
}

impl Capabilities {
    pub const NONE: Self = Self { floating: false };
}

/// The compositor this session is running under, or `Unsupported`. Never fails: a session under
/// something else still gets a value to ask, and every operation on it answers
/// [`CompositorError::Unsupported`] so the daemon degrades instead of refusing to start.
#[derive(Debug, Clone)]
pub enum Compositor {
    Niri(Niri),
    Hyprland(Hyprland),
    Unsupported,
}

/// Reads the environment only — no filesystem, no connection — so it stays cheap enough to call
/// before deciding whether to start anything.
pub fn detect_compositor() -> Compositor {
    if let Some(niri) = niri::from_env() {
        return Compositor::Niri(niri);
    }
    if let Some(hyprland) = hyprland::from_env() {
        return Compositor::Hyprland(hyprland);
    }
    Compositor::Unsupported
}

macro_rules! delegate {
    ($self:ident, $unsupported:literal, |$backend:ident| $call:expr) => {
        match $self {
            Compositor::Niri($backend) => $call.await,
            Compositor::Hyprland($backend) => $call.await,
            Compositor::Unsupported => Err(CompositorError::Unsupported($unsupported)),
        }
    };
}

impl Compositor {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Niri(_) => "niri",
            Self::Hyprland(_) => "hyprland",
            Self::Unsupported => "unsupported",
        }
    }

    pub fn capabilities(&self) -> Capabilities {
        match self {
            Self::Niri(_) => niri::CAPABILITIES,
            Self::Hyprland(_) => hyprland::CAPABILITIES,
            Self::Unsupported => Capabilities::NONE,
        }
    }

    pub async fn snapshot(&self) -> Result<Snapshot, CompositorError> {
        delegate!(self, "cannot read compositor state", |backend| backend
            .snapshot())
    }

    /// Boxed because the two backends decode different wire formats and so yield different concrete
    /// stream types. The stream ends when the compositor's socket closes, and nothing reconnects:
    /// a compositor that is gone has taken the session with it.
    pub async fn events(&self) -> Result<BoxStream<'static, Event>, CompositorError> {
        delegate!(self, "cannot follow compositor events", |backend| backend
            .events())
    }

    pub async fn switch_keyboard_layout(&self, to: LayoutTarget) -> Result<(), CompositorError> {
        delegate!(self, "cannot switch keyboard layout", |backend| backend
            .switch_keyboard_layout(to))
    }

    pub async fn focus_workspace(&self, to: WorkspaceTarget) -> Result<(), CompositorError> {
        delegate!(self, "cannot focus a workspace", |backend| backend
            .focus_workspace(to))
    }

    pub async fn rename_workspace(
        &self,
        id: WorkspaceId,
        name: Option<&str>,
    ) -> Result<(), CompositorError> {
        delegate!(self, "cannot rename a workspace", |backend| backend
            .rename_workspace(id, name))
    }

    pub async fn focus_window(&self, to: WindowTarget) -> Result<(), CompositorError> {
        delegate!(self, "cannot focus a window", |backend| backend
            .focus_window(to))
    }

    pub async fn set_output_enabled(
        &self,
        connector: &str,
        on: bool,
    ) -> Result<(), CompositorError> {
        delegate!(self, "cannot enable or disable an output", |backend| {
            backend.set_output_enabled(connector, on)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_unsupported_session_answers_every_operation_rather_than_panicking() {
        let compositor = Compositor::Unsupported;

        assert_eq!(compositor.name(), "unsupported");
        assert_eq!(compositor.capabilities(), Capabilities::NONE);
        assert!(matches!(
            compositor.snapshot().await,
            Err(CompositorError::Unsupported(_))
        ));
        assert!(matches!(
            compositor.events().await.err(),
            Some(CompositorError::Unsupported(_))
        ));
        assert!(matches!(
            compositor.focus_window(WindowTarget::Next).await,
            Err(CompositorError::Unsupported(_))
        ));
        assert!(matches!(
            compositor.set_output_enabled("eDP-1", true).await,
            Err(CompositorError::Unsupported(_))
        ));
    }

    #[test]
    fn the_two_backends_disagree_only_about_floating() {
        assert!(
            !Compositor::Niri(Niri::at("/nonexistent"))
                .capabilities()
                .floating
        );
        assert!(
            Compositor::Hyprland(Hyprland::at("/nonexistent"))
                .capabilities()
                .floating
        );
    }
}
