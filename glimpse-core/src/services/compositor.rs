use tokio::sync::{mpsc, watch};
use tokio::task::{JoinHandle, spawn_blocking};
use tokio::time::{Duration, Instant, sleep};
use tokio_util::sync::CancellationToken;

use crate::{
    compositors::night_light::WaylandNightLightController,
    compositors::{
        Compositor, CompositorCapabilities, CompositorEvent, CompositorRefresh, CompositorSnapshot,
        CompositorStructureSnapshot, CompositorType, KeyboardLayout, KeyboardLayoutSnapshot,
        Monitor, ScreencastSession, Window, Workspace, detect_compositor,
    },
    services::framework::{Control, ServiceCommand, ServiceHandle},
};

const COMMAND_QUEUE_SIZE: usize = 8;
const EVENT_QUEUE_SIZE: usize = 32;
const RETRY_DELAY: Duration = Duration::from_secs(2);
const REFRESH_DEBOUNCE: Duration = Duration::from_millis(40);
const RECOVERY_DEBOUNCE: Duration = Duration::from_millis(500);
const RECOVERY_COOLDOWN: Duration = Duration::from_secs(2);
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub compositor: CompositorType,
    pub capabilities: CompositorCapabilities,
    pub windows: Vec<Window>,
    pub workspaces: Vec<Workspace>,
    pub monitors: Vec<Monitor>,
    pub screencasts: Vec<ScreencastSession>,
    pub current_keyboard_layout: Option<usize>,
    pub focused_window: Option<usize>,
    pub current_workspace: Option<usize>,
    pub keyboard_layouts: Vec<KeyboardLayout>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Command {
    SetKeyboardLayout(usize),
    SetWorkspace(usize),
    RenameWorkspace {
        workspace: usize,
        name: Option<String>,
    },
    FocusNextWorkspace,
    FocusPreviousWorkspace,
    FocusWindow(usize),
    FocusNextWindow,
    FocusPreviousWindow,
    StopScreencast(String),
    SetNightLightTemperature(u32),
    ResetNightLight,
    SetMonitorEnabled {
        name: String,
        on: bool,
    },
}

pub type CompositorHandle = ServiceHandle<State, Command>;

#[derive(Debug)]
enum RecoveryOutcome {
    Failed,
    Succeeded,
}

pub struct CompositorService {
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
    recovery_handle: Option<JoinHandle<()>>,
    recovery_cooldown_until: Option<Instant>,
    recovery_outcome_tx: mpsc::UnboundedSender<RecoveryOutcome>,
    recovery_outcome_rx: mpsc::UnboundedReceiver<RecoveryOutcome>,
    builtin_override: Option<String>,
    night_light_controller: Option<WaylandNightLightController>,
}

enum RunOutcome {
    Cancelled,
    RetryAfterDelay,
}

impl CompositorService {
    pub fn new() -> (Self, CompositorHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);
        let (recovery_outcome_tx, recovery_outcome_rx) = mpsc::unbounded_channel();

        (
            Self {
                state_tx,
                command_rx,
                recovery_handle: None,
                recovery_cooldown_until: None,
                recovery_outcome_tx,
                recovery_outcome_rx,
                builtin_override: None,
                night_light_controller: None,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        loop {
            let outcome = match self.run_inner(cancel.clone()).await {
                Ok(outcome) => outcome,
                Err(error) => {
                    tracing::warn!(error = %error, "compositor service failed");
                    RunOutcome::RetryAfterDelay
                }
            };

            match outcome {
                RunOutcome::Cancelled => break,
                RunOutcome::RetryAfterDelay => {
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        _ = sleep(RETRY_DELAY) => {}
                    }
                }
            }
        }
    }

    async fn run_inner(&mut self, cancel: CancellationToken) -> anyhow::Result<RunOutcome> {
        let Some(compositor) = detect_compositor() else {
            self.replace_state(State::default());
            tracing::warn!("compositor service: unsupported compositor");
            return Ok(RunOutcome::RetryAfterDelay);
        };

        tracing::info!(
            compositor = compositor.name(),
            "compositor service: connected"
        );
        self.publish_identity(compositor);
        self.refresh_snapshot(compositor).await;
        self.check_all_off_and_schedule_recovery(compositor);

        let (event_tx, mut event_rx) = mpsc::channel(EVENT_QUEUE_SIZE);
        let listener = tokio::spawn(compositor.listen(event_tx));
        tokio::pin!(listener);
        let refresh_timer = sleep(REFRESH_DEBOUNCE);
        tokio::pin!(refresh_timer);
        let mut pending_refresh = None;
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    listener.abort();
                    self.cancel_recovery();
                    return Ok(RunOutcome::Cancelled);
                }
                result = &mut listener => {
                    match result {
                        Ok(Ok(())) => tracing::warn!("compositor event listener stopped"),
                        Ok(Err(error)) => tracing::warn!(error = %error, "compositor event listener failed"),
                        Err(error) if error.is_cancelled() => {}
                        Err(error) => tracing::warn!(error = %error, "compositor event listener task failed"),
                    }
                    self.cancel_recovery();
                    return Ok(RunOutcome::RetryAfterDelay);
                }
                _ = &mut refresh_timer, if pending_refresh.is_some() => {
                    if let Some(refresh) = pending_refresh.take() {
                        self.refresh(compositor, refresh).await;
                        self.check_all_off_and_schedule_recovery(compositor);
                    }
                }
                Some(outcome) = self.recovery_outcome_rx.recv() => {
                    match outcome {
                        RecoveryOutcome::Failed => {
                            self.recovery_cooldown_until = Some(Instant::now() + RECOVERY_COOLDOWN);
                        }
                        RecoveryOutcome::Succeeded => {
                            self.recovery_cooldown_until = None;
                        }
                    }
                    self.recovery_handle = None;
                }
                event = event_rx.recv() => match event {
                    Some(CompositorEvent::RefreshRequested(refresh)) => {
                        let schedule_refresh = pending_refresh.is_none();
                        pending_refresh = Some(match pending_refresh {
                            Some(pending) => pending.merge(refresh),
                            None => refresh,
                        });
                        if schedule_refresh {
                            refresh_timer.as_mut().reset(Instant::now() + REFRESH_DEBOUNCE);
                        }
                    }
                    Some(event) => {
                        self.apply_event(compositor, event).await;
                        self.check_all_off_and_schedule_recovery(compositor);
                    }
                    None => {
                        self.cancel_recovery();
                        return Ok(RunOutcome::RetryAfterDelay);
                    }
                },
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Command(command)) => {
                        self.execute_command(compositor, command).await;
                    }
                    Some(ServiceCommand::Control(control)) => match control {
                        Control::Start(config) | Control::Reconfigure(config) => {
                            self.builtin_override = config.monitors.builtin_connector.clone();
                            self.check_all_off_and_schedule_recovery(compositor);
                        }
                        Control::Shutdown => {
                            listener.abort();
                            self.cancel_recovery();
                            return Ok(RunOutcome::Cancelled);
                        }
                    },
                    None => {
                        listener.abort();
                        self.cancel_recovery();
                        return Ok(RunOutcome::Cancelled);
                    }
                },
            }
        }
    }

    async fn apply_event(&self, compositor: Compositor, event: CompositorEvent) {
        let compositor_type = compositor.compositor_type();
        self.state_tx.send_if_modified(|state| {
            if event.name() != "window-changed" {
                tracing::debug!(
                    compositor = compositor.name(),
                    event = event.name(),
                    "compositor event"
                );
            }
            let mut changed = set_if_changed(&mut state.compositor, compositor_type);
            match event {
                CompositorEvent::Snapshot(snapshot) => {
                    changed |= apply_snapshot(state, compositor_type, snapshot);
                }
                CompositorEvent::RefreshRequested(_) => {}
                CompositorEvent::WindowsChanged(windows) => {
                    changed |= set_if_changed(&mut state.windows, windows);
                    changed |= sync_focused_window_from_windows(state);
                    changed |= sync_current_workspace_from_focus_or_workspace(state);
                }
                CompositorEvent::WindowChanged(window) => {
                    changed |= apply_window_changed(state, window);
                }
                CompositorEvent::WindowTitleChanged { window, title } => {
                    if let Some(item) = state.windows.iter_mut().find(|item| item.id == window) {
                        changed |= set_if_changed(&mut item.title, Some(title));
                    }
                }
                CompositorEvent::WindowFullscreenChanged { window, fullscreen } => {
                    if let Some(window) = window.or(state.focused_window) {
                        if let Some(item) = state.windows.iter_mut().find(|item| item.id == window)
                        {
                            changed |= set_if_changed(&mut item.fullscreen, fullscreen);
                        }
                    }
                }
                CompositorEvent::WindowFloatingChanged { window, floating } => {
                    if let Some(item) = state.windows.iter_mut().find(|item| item.id == window) {
                        changed |= set_if_changed(&mut item.floating, Some(floating));
                    }
                }
                CompositorEvent::WindowClosed(window) => {
                    let len = state.windows.len();
                    state.windows.retain(|item| item.id != window);
                    changed |= state.windows.len() != len;
                    if state.focused_window == Some(window) {
                        changed |= set_if_changed(&mut state.focused_window, None);
                    }
                    changed |= mark_focused_window(&mut state.windows, state.focused_window);
                }
                CompositorEvent::WorkspacesChanged(workspaces) => {
                    changed |= set_if_changed(&mut state.workspaces, workspaces);
                    let current_workspace = state
                        .workspaces
                        .iter()
                        .find(|workspace| workspace.focused)
                        .map(|workspace| workspace.id);
                    changed |= set_if_changed(&mut state.current_workspace, current_workspace);
                }
                CompositorEvent::WorkspaceChanged { id, focused } => {
                    changed |= apply_workspace_changed(state, id, focused);
                }
                CompositorEvent::WorkspaceActiveWindowChanged { workspace, window } => {
                    if let Some(item) = state
                        .workspaces
                        .iter_mut()
                        .find(|item| item.id == workspace)
                    {
                        changed |= set_if_changed(&mut item.active_window, window);
                    }
                    if state.current_workspace == Some(workspace) {
                        changed |= set_if_changed(&mut state.focused_window, window);
                    }
                    changed |= mark_focused_window(&mut state.windows, state.focused_window);
                }
                CompositorEvent::MonitorsChanged(monitors) => {
                    changed |= set_if_changed(&mut state.monitors, monitors);
                    let current_workspace = state
                        .monitors
                        .iter()
                        .find(|monitor| monitor.focused)
                        .and_then(|monitor| monitor.active_workspace)
                        .or_else(|| {
                            state
                                .workspaces
                                .iter()
                                .find(|workspace| workspace.focused)
                                .map(|workspace| workspace.id)
                        });
                    changed |= set_if_changed(&mut state.current_workspace, current_workspace);
                }
                CompositorEvent::MonitorChanged {
                    name,
                    active_workspace,
                    focused,
                } => {
                    for monitor in &mut state.monitors {
                        if focused {
                            changed |= set_if_changed(&mut monitor.focused, monitor.name == name);
                        }
                        if monitor.name == name {
                            changed |=
                                set_if_changed(&mut monitor.active_workspace, active_workspace);
                        }
                    }
                    if focused {
                        changed |= set_if_changed(&mut state.current_workspace, active_workspace);
                    }
                }
                CompositorEvent::KeyboardLayoutsChanged { layouts, current } => {
                    changed |= set_if_changed(&mut state.keyboard_layouts, layouts);
                    changed |= set_if_changed(&mut state.current_keyboard_layout, current);
                }
                CompositorEvent::KeyboardLayoutChanged { index, name } => {
                    let current = index.or_else(|| {
                        name.as_deref().and_then(|name| {
                            state
                                .keyboard_layouts
                                .iter()
                                .position(|layout| layout.name == name)
                        })
                    });
                    if current.is_some() || index.is_some() {
                        changed |= set_if_changed(&mut state.current_keyboard_layout, current);
                    }
                }
                CompositorEvent::FocusedWindowChanged(window) => {
                    changed |= set_if_changed(&mut state.focused_window, window);
                    changed |= mark_focused_window(&mut state.windows, state.focused_window);
                    changed |= sync_current_workspace_from_focus_or_workspace(state);
                }
                CompositorEvent::ScreencastsChanged(screencasts) => {
                    changed |= apply_compositor_screencasts(&mut state.screencasts, screencasts);
                }
                CompositorEvent::ScreencastChanged(screencast) => {
                    changed |= apply_screencast_changed(state, screencast);
                }
                CompositorEvent::ScreencastStopped(id) => {
                    let len = state.screencasts.len();
                    state.screencasts.retain(|item| item.id != id);
                    changed |= state.screencasts.len() != len;
                }
            }

            changed
        });
    }

    async fn refresh(&self, compositor: Compositor, refresh: CompositorRefresh) {
        if refresh.is_full() {
            self.refresh_snapshot(compositor).await;
            return;
        }

        if refresh.includes_structure() {
            self.refresh_structure(compositor).await;
        }
        if refresh.includes_keyboard_layouts() {
            self.refresh_keyboard_layouts(compositor).await;
        }
    }

    async fn refresh_snapshot(&self, compositor: Compositor) {
        match compositor.snapshot().await {
            Ok(snapshot) => {
                let compositor_type = compositor.compositor_type();
                self.state_tx
                    .send_if_modified(|state| apply_snapshot(state, compositor_type, snapshot));
            }
            Err(error) => {
                tracing::warn!(error = %error, "failed to refresh compositor snapshot");
                self.state_tx.send_if_modified(|state| {
                    set_if_changed(&mut state.compositor, compositor.compositor_type())
                });
            }
        }
    }

    fn publish_identity(&self, compositor: Compositor) {
        let compositor_type = compositor.compositor_type();
        let capabilities = compositor.capabilities();
        self.state_tx.send_if_modified(|state| {
            let mut changed = set_if_changed(&mut state.compositor, compositor_type);
            changed |= set_if_changed(&mut state.capabilities, capabilities);
            changed
        });
    }

    async fn refresh_structure(&self, compositor: Compositor) {
        match compositor.structure_snapshot().await {
            Ok(snapshot) => {
                let compositor_type = compositor.compositor_type();
                self.state_tx.send_if_modified(|state| {
                    let mut changed = set_if_changed(&mut state.compositor, compositor_type);
                    changed |= apply_structure_snapshot(state, snapshot);
                    changed
                });
            }
            Err(error) => {
                tracing::warn!(error = %error, "failed to refresh compositor structure");
                self.refresh_snapshot(compositor).await;
            }
        }
    }

    async fn refresh_keyboard_layouts(&self, compositor: Compositor) {
        match compositor.keyboard_layout_snapshot().await {
            Ok(snapshot) => {
                let compositor_type = compositor.compositor_type();
                self.state_tx.send_if_modified(|state| {
                    let mut changed = set_if_changed(&mut state.compositor, compositor_type);
                    changed |= apply_keyboard_layout_snapshot(state, snapshot);
                    changed
                });
            }
            Err(error) => {
                tracing::warn!(error = %error, "failed to refresh compositor keyboard layouts");
                self.refresh_snapshot(compositor).await;
            }
        }
    }

    fn replace_state(&self, state: State) {
        self.state_tx
            .send_if_modified(|current| set_if_changed(current, state));
    }

    async fn execute_command(&mut self, compositor: Compositor, command: Command) {
        let result = match command {
            Command::SetKeyboardLayout(layout) => compositor.set_keyboard_layout(layout).await,
            Command::SetWorkspace(workspace) => compositor.set_workspace(workspace).await,
            Command::RenameWorkspace { workspace, name } => {
                compositor
                    .rename_workspace(workspace, name.as_deref())
                    .await
            }
            Command::FocusNextWorkspace => compositor.focus_next_workspace().await,
            Command::FocusPreviousWorkspace => compositor.focus_previous_workspace().await,
            Command::FocusWindow(window) => compositor.focus_window(window).await,
            Command::FocusNextWindow => compositor.focus_next_window().await,
            Command::FocusPreviousWindow => compositor.focus_previous_window().await,
            Command::StopScreencast(session_id) => compositor.stop_screencast(&session_id).await,
            Command::SetNightLightTemperature(temperature_kelvin) => {
                self.set_night_light_temperature(compositor, temperature_kelvin)
                    .await
            }
            Command::ResetNightLight => self.reset_night_light().await,
            Command::SetMonitorEnabled { name, on } => {
                if would_disable_last(&self.state_tx.borrow().monitors, &name, on) {
                    tracing::warn!(monitor = %name, "refusing to disable the only enabled monitor");
                    return;
                }
                compositor.set_monitor_enabled(&name, on).await
            }
        };

        if let Err(error) = result {
            tracing::warn!(error = %error, "compositor command failed");
        }
    }

    async fn set_night_light_temperature(
        &mut self,
        compositor: Compositor,
        temperature_kelvin: u32,
    ) -> anyhow::Result<()> {
        if !compositor.capabilities().night_light {
            anyhow::bail!(
                "{} does not advertise night light capability",
                compositor.name()
            );
        }

        let mut controller = match self.night_light_controller.take() {
            Some(c) => c,
            None => WaylandNightLightController::connect()?,
        };
        let (result, controller) = spawn_blocking(move || {
            let result = controller.apply_temperature(temperature_kelvin);
            (result, controller)
        })
        .await
        .map_err(|e| anyhow::anyhow!("night light worker panicked: {e}"))?;
        self.night_light_controller = Some(controller);
        result
    }

    async fn reset_night_light(&mut self) -> anyhow::Result<()> {
        if let Some(controller) = self.night_light_controller.as_mut() {
            controller.reset()?;
        }
        self.night_light_controller = None;
        Ok(())
    }

    fn cancel_recovery(&mut self) {
        if let Some(handle) = self.recovery_handle.take() {
            handle.abort();
        }
    }

    fn check_all_off_and_schedule_recovery(&mut self, compositor: Compositor) {
        let monitors = self.state_tx.borrow().monitors.clone();
        if let Some(target) =
            pick_immediate_builtin_recovery_target(&monitors, self.builtin_override.as_deref())
        {
            self.recover_monitor_now(compositor, target.to_owned());
            return;
        }

        if !should_schedule_recovery(&monitors) {
            self.cancel_recovery();
            return;
        }

        if self
            .recovery_cooldown_until
            .is_some_and(|deadline| Instant::now() < deadline)
        {
            return;
        }

        if pick_recovery_target(&monitors, self.builtin_override.as_deref()).is_none() {
            return;
        }

        self.cancel_recovery();
        let outcome_tx = self.recovery_outcome_tx.clone();
        let state_rx = self.state_tx.subscribe();
        let builtin_override = self.builtin_override.clone();
        let handle = tokio::spawn(async move {
            sleep(RECOVERY_DEBOUNCE).await;
            let monitors = state_rx.borrow().monitors.clone();
            if !should_schedule_recovery(&monitors) {
                let _ = outcome_tx.send(RecoveryOutcome::Succeeded);
                return;
            }
            let Some(target) =
                pick_recovery_target(&monitors, builtin_override.as_deref()).map(str::to_owned)
            else {
                let _ = outcome_tx.send(RecoveryOutcome::Succeeded);
                return;
            };
            tracing::warn!(monitor = %target, "all outputs disabled; re-enabling");
            match compositor.set_monitor_enabled(&target, true).await {
                Ok(()) => {
                    let _ = outcome_tx.send(RecoveryOutcome::Succeeded);
                }
                Err(error) => {
                    tracing::warn!(error = %error, monitor = %target, "recovery failed");
                    let _ = outcome_tx.send(RecoveryOutcome::Failed);
                }
            }
        });
        self.recovery_handle = Some(handle);
    }

    fn recover_monitor_now(&mut self, compositor: Compositor, target: String) {
        if self.recovery_handle.is_some() {
            return;
        }
        if self
            .recovery_cooldown_until
            .is_some_and(|deadline| Instant::now() < deadline)
        {
            return;
        }

        let outcome_tx = self.recovery_outcome_tx.clone();
        let handle = tokio::spawn(async move {
            tracing::warn!(
                monitor = %target,
                "only connected display is disabled; enabling built-in display"
            );
            match compositor.set_monitor_enabled(&target, true).await {
                Ok(()) => {
                    let _ = outcome_tx.send(RecoveryOutcome::Succeeded);
                }
                Err(error) => {
                    tracing::warn!(error = %error, monitor = %target, "immediate recovery failed");
                    let _ = outcome_tx.send(RecoveryOutcome::Failed);
                }
            }
        });
        self.recovery_handle = Some(handle);
    }
}

fn should_schedule_recovery(monitors: &[Monitor]) -> bool {
    !monitors.is_empty() && monitors.iter().all(|m| !m.enabled)
}

fn pick_immediate_builtin_recovery_target<'a>(
    monitors: &'a [Monitor],
    builtin_override: Option<&str>,
) -> Option<&'a str> {
    let [monitor] = monitors else {
        return None;
    };
    if monitor.enabled {
        return None;
    }
    if monitor.built_in || builtin_override == Some(monitor.name.as_str()) {
        return Some(monitor.name.as_str());
    }
    None
}

/// Priority order: explicit override -> first `built_in==true` by name -> first by name alphabetical.
fn pick_recovery_target<'a>(
    monitors: &'a [Monitor],
    builtin_override: Option<&str>,
) -> Option<&'a str> {
    if monitors.is_empty() {
        return None;
    }
    let mut sorted: Vec<&Monitor> = monitors.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    if let Some(override_name) = builtin_override
        && let Some(m) = sorted.iter().find(|m| m.name == override_name)
    {
        return Some(m.name.as_str());
    }
    sorted
        .iter()
        .find(|m| m.built_in)
        .or_else(|| sorted.first())
        .map(|m| m.name.as_str())
}

fn would_disable_last(monitors: &[Monitor], name: &str, on: bool) -> bool {
    if on {
        return false;
    }
    let enabled_count = monitors.iter().filter(|m| m.enabled).count();
    if enabled_count != 1 {
        return false;
    }
    monitors.iter().any(|m| m.name == name && m.enabled)
}

fn apply_snapshot(
    state: &mut State,
    compositor: CompositorType,
    snapshot: CompositorSnapshot,
) -> bool {
    let CompositorSnapshot {
        capabilities,
        windows,
        workspaces,
        monitors,
        screencasts,
        keyboard_layouts,
        current_keyboard_layout,
        focused_window,
        current_workspace,
    } = snapshot;
    let mut changed = set_if_changed(&mut state.compositor, compositor);
    changed |= set_if_changed(&mut state.capabilities, capabilities);
    changed |= apply_compositor_screencasts(&mut state.screencasts, screencasts);
    changed |= apply_structure_snapshot(
        state,
        CompositorStructureSnapshot {
            windows,
            workspaces,
            monitors,
            focused_window,
            current_workspace,
        },
    );
    changed |= apply_keyboard_layout_snapshot(
        state,
        KeyboardLayoutSnapshot {
            keyboard_layouts,
            current_keyboard_layout,
        },
    );
    changed
}

fn apply_screencast_changed(state: &mut State, screencast: ScreencastSession) -> bool {
    if !screencast.active {
        let len = state.screencasts.len();
        state.screencasts.retain(|item| item.id != screencast.id);
        return state.screencasts.len() != len;
    }

    if let Some(existing) = state
        .screencasts
        .iter_mut()
        .find(|item| item.id == screencast.id)
    {
        return set_if_changed(existing, screencast);
    }

    state.screencasts.push(screencast);
    true
}

fn apply_compositor_screencasts(
    screencasts: &mut Vec<ScreencastSession>,
    compositor: Vec<ScreencastSession>,
) -> bool {
    let original = screencasts.clone();
    *screencasts = compositor;
    screencasts.sort_by(|left, right| left.id.cmp(&right.id));
    *screencasts != original
}

fn apply_structure_snapshot(state: &mut State, snapshot: CompositorStructureSnapshot) -> bool {
    let mut changed = set_if_changed(&mut state.windows, snapshot.windows);
    changed |= set_if_changed(&mut state.workspaces, snapshot.workspaces);
    changed |= set_if_changed(&mut state.monitors, snapshot.monitors);
    changed |= set_if_changed(&mut state.focused_window, snapshot.focused_window);
    changed |= set_if_changed(&mut state.current_workspace, snapshot.current_workspace);
    changed
}

fn apply_keyboard_layout_snapshot(state: &mut State, snapshot: KeyboardLayoutSnapshot) -> bool {
    let mut changed = set_if_changed(&mut state.keyboard_layouts, snapshot.keyboard_layouts);
    changed |= set_if_changed(
        &mut state.current_keyboard_layout,
        snapshot.current_keyboard_layout,
    );
    changed
}

fn apply_window_changed(state: &mut State, window: Window) -> bool {
    let focused = window.focused;
    let workspace = window.workspace;
    let window_id = window.id;
    let was_focused = state.focused_window == Some(window_id);
    let mut changed = upsert_by_id(&mut state.windows, window, |window| window.id);

    if focused {
        changed |= set_if_changed(&mut state.focused_window, Some(window_id));
        changed |= mark_focused_window(&mut state.windows, state.focused_window);
        if let Some(workspace) = workspace {
            changed |= set_if_changed(&mut state.current_workspace, Some(workspace));
        }
    } else if was_focused {
        changed |= mark_focused_window(&mut state.windows, state.focused_window);
        changed |= sync_current_workspace_from_focus_or_workspace(state);
    }

    changed
}

fn set_if_changed<T>(slot: &mut T, value: T) -> bool
where
    T: PartialEq,
{
    if *slot == value {
        false
    } else {
        *slot = value;
        true
    }
}

fn upsert_by_id<T, F>(items: &mut Vec<T>, item: T, id: F) -> bool
where
    T: PartialEq,
    F: Fn(&T) -> usize,
{
    let item_id = id(&item);
    match items.iter().position(|existing| id(existing) == item_id) {
        Some(index) if items[index] != item => {
            items[index] = item;
            true
        }
        Some(_) => false,
        None => {
            items.push(item);
            true
        }
    }
}

fn mark_focused_window(windows: &mut [Window], focused_window: Option<usize>) -> bool {
    let mut changed = false;
    for window in windows {
        changed |= set_if_changed(&mut window.focused, Some(window.id) == focused_window);
    }
    changed
}

fn sync_focused_window_from_windows(state: &mut State) -> bool {
    let focused_window = state
        .windows
        .iter()
        .find(|window| window.focused)
        .map(|window| window.id);
    set_if_changed(&mut state.focused_window, focused_window)
}

fn apply_workspace_changed(state: &mut State, id: usize, focused: bool) -> bool {
    let mut changed = false;
    if focused {
        changed |= set_if_changed(&mut state.current_workspace, Some(id));
    }
    let activated_monitor = state
        .workspaces
        .iter()
        .find(|workspace| workspace.id == id)
        .and_then(|workspace| workspace.monitor.clone());
    for workspace in &mut state.workspaces {
        if focused {
            changed |= set_if_changed(&mut workspace.focused, workspace.id == id);
        }
        if workspace.id == id {
            changed |= set_if_changed(&mut workspace.active, true);
        } else if activated_monitor.is_some() && workspace.monitor == activated_monitor {
            changed |= set_if_changed(&mut workspace.active, false);
        }
    }
    if let Some(monitor_name) = activated_monitor.as_deref() {
        for monitor in &mut state.monitors {
            if monitor.name == monitor_name {
                changed |= set_if_changed(&mut monitor.active_workspace, Some(id));
            }
            if focused {
                changed |= set_if_changed(&mut monitor.focused, monitor.name == monitor_name);
            }
        }
    }
    changed
}

fn sync_current_workspace_from_focus_or_workspace(state: &mut State) -> bool {
    let current_workspace = state
        .focused_window
        .and_then(|focused| state.windows.iter().find(|window| window.id == focused))
        .and_then(|window| window.workspace)
        .or_else(|| {
            state
                .workspaces
                .iter()
                .find(|workspace| workspace.focused)
                .map(|workspace| workspace.id)
        });
    set_if_changed(&mut state.current_workspace, current_workspace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compositors::{ScreencastKind, ScreencastTarget, niri::Niri};

    #[test]
    fn publishes_compositor_identity_before_snapshot_data() {
        let (service, handle) = CompositorService::new();

        service.publish_identity(Compositor::Niri(Niri));

        let state = handle.snapshot();
        assert_eq!(state.compositor, CompositorType::Niri);
        assert!(state.capabilities.workspaces);
        assert!(state.capabilities.windows);
    }

    #[test]
    fn applies_structure_snapshot_only_updates_structure_state() {
        let mut state = State {
            compositor: CompositorType::Hyprland,
            capabilities: CompositorCapabilities {
                windows: true,
                workspaces: true,
                monitors: true,
                keyboard_layouts: true,
                focused_window: true,
                current_workspace: true,
                fullscreen: true,
                floating: true,
                window_titles: true,
                night_light: true,
                screencast_state: crate::compositors::ScreencastStateCapability::None,
                screencast_control: crate::compositors::ScreencastControlCapability::None,
            },
            ..State::default()
        };
        let snapshot = CompositorStructureSnapshot {
            windows: vec![window(1, true, Some(3))],
            workspaces: vec![workspace(3, true)],
            monitors: vec![monitor("DP-1", Some(3), true)],
            focused_window: Some(1),
            current_workspace: Some(3),
        };

        assert!(apply_structure_snapshot(&mut state, snapshot.clone()));
        assert!(state.capabilities.night_light);
        assert!(!apply_structure_snapshot(&mut state, snapshot));
    }

    #[test]
    fn focus_helpers_only_report_real_changes() {
        let mut state = State {
            windows: vec![window(1, false, Some(1)), window(2, false, Some(4))],
            workspaces: vec![workspace(4, true)],
            focused_window: Some(2),
            current_workspace: Some(1),
            ..State::default()
        };

        assert!(mark_focused_window(
            &mut state.windows,
            state.focused_window
        ));
        assert!(!mark_focused_window(
            &mut state.windows,
            state.focused_window
        ));
        assert!(sync_current_workspace_from_focus_or_workspace(&mut state));
        assert_eq!(state.current_workspace, Some(4));
        assert!(!sync_current_workspace_from_focus_or_workspace(&mut state));
    }

    #[test]
    fn current_focused_window_update_preserves_focus_marker() {
        let mut state = State {
            windows: vec![window(7, true, Some(1))],
            focused_window: Some(7),
            current_workspace: Some(1),
            ..State::default()
        };
        let update = window(7, false, Some(2));

        assert!(apply_window_changed(&mut state, update));
        assert_eq!(state.focused_window, Some(7));
        assert_eq!(state.current_workspace, Some(2));
        assert!(state.windows[0].focused);
    }

    #[test]
    fn set_if_changed_suppresses_noop_updates() {
        let mut value = Some(1);

        assert!(!set_if_changed(&mut value, Some(1)));
        assert!(set_if_changed(&mut value, Some(2)));
        assert_eq!(value, Some(2));
    }

    #[test]
    fn workspace_changed_clears_sibling_active_and_syncs_monitor_active_workspace() {
        let mut state = State {
            workspaces: vec![
                Workspace {
                    id: 1,
                    monitor: Some("eDP-1".into()),
                    active: true,
                    focused: true,
                    ..workspace(1, false)
                },
                Workspace {
                    id: 2,
                    monitor: Some("eDP-1".into()),
                    active: false,
                    focused: false,
                    ..workspace(2, false)
                },
                Workspace {
                    id: 3,
                    monitor: Some("HDMI-A-1".into()),
                    active: true,
                    focused: false,
                    ..workspace(3, false)
                },
            ],
            monitors: vec![
                monitor("eDP-1", Some(1), true),
                monitor("HDMI-A-1", Some(3), false),
            ],
            current_workspace: Some(1),
            ..State::default()
        };

        assert!(apply_workspace_changed(&mut state, 2, true));

        assert_eq!(state.current_workspace, Some(2));
        assert!(
            !state.workspaces[0].active,
            "previous active sibling cleared"
        );
        assert!(state.workspaces[1].active, "newly activated marked active");
        assert!(
            state.workspaces[2].active,
            "other monitor's active untouched"
        );
        assert!(!state.workspaces[0].focused);
        assert!(state.workspaces[1].focused);
        assert_eq!(state.monitors[0].active_workspace, Some(2));
        assert_eq!(state.monitors[1].active_workspace, Some(3));
        assert!(
            state.monitors[0].focused,
            "monitor owning the focused workspace stays focused"
        );
    }

    #[test]
    fn workspace_changed_moves_monitor_focus_when_focus_crosses_outputs() {
        let mut state = State {
            workspaces: vec![
                Workspace {
                    id: 1,
                    monitor: Some("eDP-1".into()),
                    active: true,
                    focused: true,
                    ..workspace(1, true)
                },
                Workspace {
                    id: 3,
                    monitor: Some("HDMI-A-1".into()),
                    active: true,
                    focused: false,
                    ..workspace(3, false)
                },
            ],
            monitors: vec![
                monitor("eDP-1", Some(1), true),
                monitor("HDMI-A-1", Some(3), false),
            ],
            current_workspace: Some(1),
            ..State::default()
        };

        assert!(apply_workspace_changed(&mut state, 3, true));

        assert_eq!(state.current_workspace, Some(3));
        assert!(state.workspaces[1].focused);
        assert!(!state.workspaces[0].focused);
        assert!(
            !state.monitors[0].focused,
            "previous focused monitor must lose its focused flag"
        );
        assert!(
            state.monitors[1].focused,
            "monitor owning the newly focused workspace must become focused"
        );
    }

    #[test]
    fn workspace_changed_unfocused_preserves_global_focus_and_updates_per_monitor_active() {
        let mut state = State {
            workspaces: vec![
                Workspace {
                    id: 1,
                    monitor: Some("eDP-1".into()),
                    active: true,
                    focused: false,
                    ..workspace(1, false)
                },
                Workspace {
                    id: 2,
                    monitor: Some("eDP-1".into()),
                    active: false,
                    focused: false,
                    ..workspace(2, false)
                },
                Workspace {
                    id: 3,
                    monitor: Some("HDMI-A-1".into()),
                    active: true,
                    focused: true,
                    ..workspace(3, true)
                },
            ],
            monitors: vec![
                monitor("eDP-1", Some(1), false),
                monitor("HDMI-A-1", Some(3), true),
            ],
            current_workspace: Some(3),
            ..State::default()
        };

        assert!(apply_workspace_changed(&mut state, 2, false));

        assert_eq!(
            state.current_workspace,
            Some(3),
            "global focus must not move when activation is on a non-focused output"
        );
        assert!(state.workspaces[2].focused, "global focus marker preserved");
        assert!(!state.workspaces[0].active);
        assert!(state.workspaces[1].active);
        assert!(state.workspaces[2].active);
        assert_eq!(state.monitors[0].active_workspace, Some(2));
        assert_eq!(state.monitors[1].active_workspace, Some(3));
    }

    #[test]
    fn compositor_screencast_replacement_replaces_previous_compositor_sessions() {
        let mut screencasts = vec![screencast("old-niri")];

        assert!(apply_compositor_screencasts(
            &mut screencasts,
            vec![screencast("new-niri")]
        ));

        assert_eq!(screencasts, vec![screencast("new-niri")]);
    }

    #[test]
    fn would_disable_last_returns_true_when_target_is_only_enabled_monitor() {
        let mut other = monitor("HDMI-A-1", None, false);
        other.enabled = false;
        let monitors = vec![monitor("DP-1", None, true), other];

        assert!(would_disable_last(&monitors, "DP-1", false));
    }

    #[test]
    fn would_disable_last_returns_false_when_another_monitor_is_enabled() {
        let monitors = vec![
            monitor("DP-1", None, true),
            monitor("HDMI-A-1", None, false),
        ];

        assert!(!would_disable_last(&monitors, "DP-1", false));
    }

    #[test]
    fn would_disable_last_returns_false_when_command_is_enabling() {
        let mut m = monitor("DP-1", None, false);
        m.enabled = false;
        let monitors = vec![m, monitor("HDMI-A-1", None, true)];

        assert!(!would_disable_last(&monitors, "DP-1", true));
    }

    #[test]
    fn would_disable_last_returns_false_when_target_is_already_disabled() {
        let mut m = monitor("DP-1", None, false);
        m.enabled = false;
        let monitors = vec![m, monitor("HDMI-A-1", None, true)];

        assert!(!would_disable_last(&monitors, "DP-1", false));
    }

    #[test]
    fn should_schedule_recovery_true_when_all_disabled() {
        let mut a = monitor("DP-1", None, false);
        a.enabled = false;
        let mut b = monitor("HDMI-A-1", None, false);
        b.enabled = false;
        assert!(should_schedule_recovery(&[a, b]));
    }

    #[test]
    fn should_schedule_recovery_false_when_any_enabled() {
        let mut a = monitor("DP-1", None, false);
        a.enabled = false;
        let b = monitor("HDMI-A-1", None, false);
        assert!(!should_schedule_recovery(&[a, b]));
    }

    #[test]
    fn should_schedule_recovery_false_when_no_monitors() {
        assert!(!should_schedule_recovery(&[]));
    }

    #[test]
    fn pick_recovery_target_prefers_builtin() {
        let mut a = monitor("DP-1", None, false);
        a.enabled = false;
        let mut b = monitor("eDP-1", None, false);
        b.enabled = false;
        b.built_in = true;
        let mut c = monitor("HDMI-A-1", None, false);
        c.enabled = false;
        assert_eq!(pick_recovery_target(&[a, b, c], None), Some("eDP-1"));
    }

    #[test]
    fn pick_recovery_target_falls_back_to_first_alphabetical() {
        let mut a = monitor("HDMI-A-1", None, false);
        a.enabled = false;
        let mut b = monitor("DP-2", None, false);
        b.enabled = false;
        let mut c = monitor("DP-1", None, false);
        c.enabled = false;
        assert_eq!(pick_recovery_target(&[a, b, c], None), Some("DP-1"));
    }

    #[test]
    fn pick_recovery_target_honours_explicit_override() {
        let mut a = monitor("DP-1", None, false);
        a.enabled = false;
        let mut b = monitor("eDP-1", None, false);
        b.enabled = false;
        b.built_in = true;
        let mut c = monitor("HDMI-A-1", None, false);
        c.enabled = false;
        assert_eq!(
            pick_recovery_target(&[a, b, c], Some("HDMI-A-1")),
            Some("HDMI-A-1")
        );
    }

    #[test]
    fn pick_recovery_target_returns_none_for_empty() {
        assert_eq!(pick_recovery_target(&[], None), None);
    }

    #[test]
    fn pick_immediate_builtin_recovery_target_selects_disabled_only_connected_builtin() {
        let mut builtin = monitor("eDP-1", None, false);
        builtin.enabled = false;
        builtin.built_in = true;

        assert_eq!(
            pick_immediate_builtin_recovery_target(&[builtin], None),
            Some("eDP-1")
        );
    }

    #[test]
    fn pick_immediate_builtin_recovery_target_waits_when_external_is_still_connected() {
        let mut builtin = monitor("eDP-1", None, false);
        builtin.enabled = false;
        builtin.built_in = true;
        let external = monitor("DP-2", None, false);

        assert_eq!(
            pick_immediate_builtin_recovery_target(&[builtin, external], None),
            None
        );
    }

    #[test]
    fn pick_immediate_builtin_recovery_target_honours_explicit_builtin_override() {
        let mut builtin = monitor("DP-3", None, false);
        builtin.enabled = false;

        assert_eq!(
            pick_immediate_builtin_recovery_target(&[builtin], Some("DP-3")),
            Some("DP-3")
        );
    }

    #[test]
    fn pick_immediate_builtin_recovery_target_ignores_enabled_builtin() {
        let mut builtin = monitor("eDP-1", None, false);
        builtin.built_in = true;

        assert_eq!(
            pick_immediate_builtin_recovery_target(&[builtin], None),
            None
        );
    }

    fn window(id: usize, focused: bool, workspace: Option<usize>) -> Window {
        Window {
            id,
            title: None,
            app_id: None,
            pid: None,
            layout_order: None,
            workspace,
            focused,
            urgent: false,
            fullscreen: false,
            floating: None,
        }
    }

    fn workspace(id: usize, focused: bool) -> Workspace {
        Workspace {
            id,
            index: Some(id),
            name: None,
            monitor: None,
            active: focused,
            focused,
            urgent: false,
            active_window: None,
        }
    }

    fn monitor(name: &str, active_workspace: Option<usize>, focused: bool) -> Monitor {
        Monitor {
            id: None,
            name: name.into(),
            description: None,
            active_workspace,
            focused,
            make: None,
            model: None,
            enabled: true,
            built_in: false,
            current_mode: None,
        }
    }

    fn screencast(id: &str) -> ScreencastSession {
        ScreencastSession {
            id: id.into(),
            session_id: None,
            kind: ScreencastKind::PipeWire,
            target: ScreencastTarget::Monitor,
            active: true,
            pipewire_node: None,
            client_pid: None,
            stoppable: false,
        }
    }
}
