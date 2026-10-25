use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;

use futures_util::stream::BoxStream;

use crate::contract::{Capability, Error};
use crate::event::Event;
use crate::model::{KeyboardLayouts, Rect, Snapshot, WindowId, WorkspaceId};

pub trait Backend: Send + Sync + 'static {
    const NAME: &'static str;
    const CAPABILITIES: &'static [Capability];

    fn snapshot(&self) -> impl Future<Output = Result<Snapshot, Error>> + Send;
    fn keyboard_layouts(&self) -> impl Future<Output = Result<KeyboardLayouts, Error>> + Send;
    fn session_lock_restore(&self) -> impl Future<Output = Result<Option<bool>, Error>> + Send;
    fn events(&self) -> impl Future<Output = Result<BoxStream<'static, Event>, Error>> + Send;
    fn focus_workspace(&self, id: WorkspaceId) -> impl Future<Output = Result<(), Error>> + Send;
    fn step_workspace(&self, forward: bool) -> impl Future<Output = Result<(), Error>> + Send;
    fn rename_workspace(
        &self,
        id: WorkspaceId,
        name: String,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn focus_window(&self, id: WindowId) -> impl Future<Output = Result<(), Error>> + Send;
    fn step_window(&self, forward: bool) -> impl Future<Output = Result<(), Error>> + Send;
    fn switch_keyboard_layout(&self, index: u32) -> impl Future<Output = Result<(), Error>> + Send;
    fn step_keyboard_layout(&self, forward: bool)
    -> impl Future<Output = Result<(), Error>> + Send;
    fn set_output_enabled(
        &self,
        connector: String,
        enabled: bool,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn power_off_monitors(&self) -> impl Future<Output = Result<(), Error>> + Send;
    fn stop_screencast(&self, session: u64) -> impl Future<Output = Result<(), Error>> + Send;
    fn screenshot_window(
        &self,
        id: WindowId,
        path: PathBuf,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn window_geometries(
        &self,
    ) -> impl Future<Output = Result<HashMap<WindowId, Rect>, Error>> + Send;
    fn probe(&self) -> impl Future<Output = bool> + Send;
}
