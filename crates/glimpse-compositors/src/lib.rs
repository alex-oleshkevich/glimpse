//! Compositor state and control for niri and Hyprland, behind one model.
//!
//! Both compositors are reached over their own Unix socket — this crate touches no `wl_` object,
//! so the daemon-era `wayland/` rule does not apply to it.

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
    Cast, CastKind, CastTarget, KeyboardLayouts, LayoutTarget, Logical, Mode, Output, Rect,
    Snapshot, Window, WindowId, WindowTarget, Workspace, WorkspaceId, WorkspaceTarget,
};
pub use niri::Niri;

use std::path::Path;

use futures_util::stream::BoxStream;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub output_power: bool,
    pub blank_screens: bool,
    pub window_screenshot: bool,
    pub overview_backdrop: bool,
}

impl Capabilities {
    pub const NONE: Self = Self {
        output_power: false,
        blank_screens: false,
        window_screenshot: false,
        overview_backdrop: false,
    };
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
    if let Some(hyprland) = Hyprland::from_env() {
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

    pub async fn keyboard_layouts(&self) -> Result<KeyboardLayouts, CompositorError> {
        delegate!(self, "cannot read keyboard layouts", |backend| backend
            .keyboard_layouts())
    }

    pub async fn allows_session_lock_restore(&self) -> Result<bool, CompositorError> {
        match self {
            Self::Hyprland(backend) => backend.allow_session_lock_restore().await,
            Self::Niri(_) | Self::Unsupported => Ok(true),
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

    pub async fn power_off_monitors(&self) -> Result<(), CompositorError> {
        delegate!(self, "cannot power off monitors", |backend| backend
            .power_off_monitors())
    }

    pub async fn stop_screencast(&self, session_id: u64) -> Result<(), CompositorError> {
        delegate!(self, "cannot stop a screencast", |backend| backend
            .stop_screencast(session_id))
    }

    pub async fn screenshot_window(
        &self,
        id: WindowId,
        path: &Path,
    ) -> Result<(), CompositorError> {
        delegate!(self, "cannot screenshot a window", |backend| backend
            .screenshot_window(id, path))
    }
}

#[cfg(test)]
mod tests;
