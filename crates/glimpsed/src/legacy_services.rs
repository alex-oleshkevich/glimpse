use std::future::Future;

use glimpse_contracts::{
    CalendarEvents, CalendarRefresh, CalendarSetRange, CloseWindow, Command as _,
    CompositorOutputs, CompositorPrivacy, CompositorStatus, CompositorWindows,
    CompositorWorkspaces, FocusOutput, FocusWindow, FocusWorkspace, GeolocationRefresh,
    GeolocationStatus, HeartbeatReset, HeartbeatSetInterval, HeartbeatTick, KeyboardLayouts,
    Message as _, MoveWindowToWorkspace, MoveWorkspaceToOutput, MprisControl, MprisPlayers,
    MprisSeek, MprisSetPosition, MprisSetRepeat, MprisSetShuffle, MprisSetVolume,
    NotificationsActivate, NotificationsClearAll, NotificationsClearApp, NotificationsDismiss,
    NotificationsDnd, NotificationsInvokeAction, NotificationsList, NotificationsRemove,
    NotificationsSetDnd, RenameWorkspace, ReorderWorkspace, SessionStatus, SolarRefresh,
    SolarStatus, SwitchLayout, WeatherRefresh, WeatherStatus, WeatherWatch,
};
use glimpse_ipc::{CallError, ErrorCode};
use glimpse_services::{
    Calendar, CalendarHandle, CommandError, Compositor, CompositorHandle, CompositorState,
    Geolocation, GeolocationHandle, Heartbeat, HeartbeatHandle, Keyboard, KeyboardHandle, Mpris,
    MprisHandle, Notifications, NotificationsHandle, NotificationsState, Service, Session,
    SessionHandle, Solar, SolarHandle, Weather, WeatherHandle,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use tokio::sync::watch;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::broker::{Dispatch, Handle, Message, Responder};

pub fn geolocation(
    tasks: &TaskTracker,
    broker: &Handle,
    cancel: CancellationToken,
    handle: GeolocationHandle,
) {
    let dispatch = one(tasks, handle.clone(), |handle, method, args| async move {
        match method.as_str() {
            GeolocationRefresh::NAME => {
                let _: GeolocationRefresh = decode(args)?;
                handle.refresh().await
            }
            _ => Err(unknown(Geolocation::NAME, &method)),
        }
    });
    declare(
        broker,
        Geolocation::NAME,
        &[GeolocationStatus::NAME],
        &[GeolocationRefresh::NAME],
        dispatch,
    );
    follow_health(
        tasks,
        broker,
        cancel.clone(),
        Geolocation::NAME,
        handle.health(),
    );
    follow(
        tasks,
        broker,
        cancel,
        handle.subscribe(),
        |broker, state| {
            publish(&broker, GeolocationStatus::NAME, state);
        },
    );
}

pub fn solar(tasks: &TaskTracker, broker: &Handle, cancel: CancellationToken, handle: SolarHandle) {
    let dispatch = one(tasks, handle.clone(), |handle, method, args| async move {
        match method.as_str() {
            SolarRefresh::NAME => {
                let _: SolarRefresh = decode(args)?;
                handle.refresh().await
            }
            _ => Err(unknown(Solar::NAME, &method)),
        }
    });
    declare(
        broker,
        Solar::NAME,
        &[SolarStatus::NAME],
        &[SolarRefresh::NAME],
        dispatch,
    );
    follow_health(tasks, broker, cancel.clone(), Solar::NAME, handle.health());
    follow(
        tasks,
        broker,
        cancel,
        handle.subscribe(),
        |broker, state| {
            if let Some(state) = state {
                publish(&broker, SolarStatus::NAME, state);
            }
        },
    );
}

pub fn heartbeat(
    tasks: &TaskTracker,
    broker: &Handle,
    cancel: CancellationToken,
    handle: HeartbeatHandle,
) {
    let commands = tasks.clone();
    let command_handle = handle.clone();
    let dispatch: Dispatch = Box::new(move |method, args, responder| match method {
        HeartbeatReset::NAME => match decode::<HeartbeatReset>(args) {
            Ok(_) => {
                let handle = command_handle.clone();
                answer(&commands, responder, async move { handle.reset().await });
            }
            Err(error) => responder.fail(to_call_error(error)),
        },
        HeartbeatSetInterval::NAME => match decode::<HeartbeatSetInterval>(args) {
            Ok(asked) => {
                let handle = command_handle.clone();
                answer(&commands, responder, async move {
                    handle.set_interval(asked.period_ms).await
                });
            }
            Err(error) => responder.fail(to_call_error(error)),
        },
        _ => responder.fail(CallError::new(
            ErrorCode::UnknownCommand,
            format!("`{}` does not answer `{method}`", Heartbeat::NAME),
        )),
    });
    declare(
        broker,
        Heartbeat::NAME,
        &[HeartbeatTick::NAME],
        &[HeartbeatReset::NAME, HeartbeatSetInterval::NAME],
        dispatch,
    );
    follow_health(
        tasks,
        broker,
        cancel.clone(),
        Heartbeat::NAME,
        handle.health(),
    );
    follow(
        tasks,
        broker,
        cancel,
        handle.subscribe(),
        |broker, state| {
            publish(&broker, HeartbeatTick::NAME, state);
        },
    );
}

pub fn compositor(
    tasks: &TaskTracker,
    broker: &Handle,
    cancel: CancellationToken,
    handle: CompositorHandle,
) {
    let dispatch = one(tasks, handle.clone(), |handle, method, args| async move {
        match method.as_str() {
            FocusWorkspace::NAME => {
                let FocusWorkspace { target } = decode(args)?;
                handle.focus_workspace(target).await
            }
            FocusWindow::NAME => {
                let FocusWindow { target } = decode(args)?;
                handle.focus_window(target).await
            }
            FocusOutput::NAME => {
                let FocusOutput { connector } = decode(args)?;
                handle.focus_output(connector).await
            }
            RenameWorkspace::NAME => {
                let RenameWorkspace { id, name } = decode(args)?;
                handle.rename_workspace(id, name).await
            }
            MoveWorkspaceToOutput::NAME => {
                let MoveWorkspaceToOutput { id, connector } = decode(args)?;
                handle.move_workspace_to_output(id, connector).await
            }
            ReorderWorkspace::NAME => {
                let ReorderWorkspace { id, index } = decode(args)?;
                handle.reorder_workspace(id, index).await
            }
            MoveWindowToWorkspace::NAME => {
                let MoveWindowToWorkspace { window, workspace } = decode(args)?;
                handle.move_window_to_workspace(window, workspace).await
            }
            CloseWindow::NAME => {
                let CloseWindow { id } = decode(args)?;
                handle.close_window(id).await
            }
            _ => Err(unknown(Compositor::NAME, &method)),
        }
    });
    declare(
        broker,
        Compositor::NAME,
        &[
            CompositorStatus::NAME,
            CompositorWorkspaces::NAME,
            CompositorWindows::NAME,
            CompositorOutputs::NAME,
            CompositorPrivacy::NAME,
        ],
        &[
            FocusWorkspace::NAME,
            FocusWindow::NAME,
            FocusOutput::NAME,
            RenameWorkspace::NAME,
            MoveWorkspaceToOutput::NAME,
            ReorderWorkspace::NAME,
            MoveWindowToWorkspace::NAME,
            CloseWindow::NAME,
        ],
        dispatch,
    );
    follow_health(
        tasks,
        broker,
        cancel.clone(),
        Compositor::NAME,
        handle.health(),
    );
    follow(
        tasks,
        broker,
        cancel,
        handle.subscribe(),
        publish_compositor,
    );
}

pub fn keyboard(
    tasks: &TaskTracker,
    broker: &Handle,
    cancel: CancellationToken,
    handle: KeyboardHandle,
) {
    let dispatch = one(tasks, handle.clone(), |handle, method, args| async move {
        match method.as_str() {
            SwitchLayout::NAME => {
                let SwitchLayout { target } = decode(args)?;
                handle.switch(target).await
            }
            _ => Err(unknown(Keyboard::NAME, &method)),
        }
    });
    declare(
        broker,
        Keyboard::NAME,
        &[KeyboardLayouts::NAME],
        &[SwitchLayout::NAME],
        dispatch,
    );
    follow_health(
        tasks,
        broker,
        cancel.clone(),
        Keyboard::NAME,
        handle.health(),
    );
    follow(
        tasks,
        broker,
        cancel,
        handle.subscribe(),
        |broker, state| {
            publish(&broker, KeyboardLayouts::NAME, state);
        },
    );
}

pub fn calendar(
    tasks: &TaskTracker,
    broker: &Handle,
    cancel: CancellationToken,
    handle: CalendarHandle,
) {
    let dispatch = one(tasks, handle.clone(), |handle, method, args| async move {
        match method.as_str() {
            CalendarRefresh::NAME => {
                let _: CalendarRefresh = decode(args)?;
                handle.refresh().await
            }
            CalendarSetRange::NAME => {
                let CalendarSetRange { from, to } = decode(args)?;
                handle.set_range(from, to).await
            }
            _ => Err(unknown(Calendar::NAME, &method)),
        }
    });
    declare(
        broker,
        Calendar::NAME,
        &[CalendarEvents::NAME],
        &[CalendarRefresh::NAME, CalendarSetRange::NAME],
        dispatch,
    );
    follow_health(
        tasks,
        broker,
        cancel.clone(),
        Calendar::NAME,
        handle.health(),
    );
    follow(
        tasks,
        broker,
        cancel,
        handle.subscribe(),
        |broker, state| {
            publish(&broker, CalendarEvents::NAME, state);
        },
    );
}

pub fn weather(
    tasks: &TaskTracker,
    broker: &Handle,
    cancel: CancellationToken,
    handle: WeatherHandle,
) {
    let dispatch = one(tasks, handle.clone(), |handle, method, args| async move {
        match method.as_str() {
            WeatherWatch::NAME => {
                let WeatherWatch { place } = decode(args)?;
                handle.watch(place).await
            }
            WeatherRefresh::NAME => {
                let _: WeatherRefresh = decode(args)?;
                handle.refresh().await
            }
            _ => Err(unknown(Weather::NAME, &method)),
        }
    });
    declare(
        broker,
        Weather::NAME,
        &[WeatherStatus::NAME],
        &[WeatherWatch::NAME, WeatherRefresh::NAME],
        dispatch,
    );
    follow_health(
        tasks,
        broker,
        cancel.clone(),
        Weather::NAME,
        handle.health(),
    );
    follow(
        tasks,
        broker,
        cancel,
        handle.subscribe(),
        |broker, state| {
            publish(&broker, WeatherStatus::NAME, state);
        },
    );
}

pub fn mpris(tasks: &TaskTracker, broker: &Handle, cancel: CancellationToken, handle: MprisHandle) {
    let dispatch = one(tasks, handle.clone(), |handle, method, args| async move {
        match method.as_str() {
            MprisControl::NAME => {
                let MprisControl { player, action } = decode(args)?;
                handle.control(player, action).await
            }
            MprisSeek::NAME => {
                let MprisSeek { player, offset_us } = decode(args)?;
                handle.seek(player, offset_us).await
            }
            MprisSetPosition::NAME => {
                let MprisSetPosition {
                    player,
                    position_us,
                } = decode(args)?;
                handle.set_position(player, position_us).await
            }
            MprisSetVolume::NAME => {
                let MprisSetVolume { player, volume } = decode(args)?;
                handle.set_volume(player, volume).await
            }
            MprisSetRepeat::NAME => {
                let MprisSetRepeat { player, repeat } = decode(args)?;
                handle.set_repeat(player, repeat).await
            }
            MprisSetShuffle::NAME => {
                let MprisSetShuffle { player, shuffle } = decode(args)?;
                handle.set_shuffle(player, shuffle).await
            }
            _ => Err(unknown(Mpris::NAME, &method)),
        }
    });
    declare(
        broker,
        Mpris::NAME,
        &[MprisPlayers::NAME],
        &[
            MprisControl::NAME,
            MprisSeek::NAME,
            MprisSetPosition::NAME,
            MprisSetVolume::NAME,
            MprisSetRepeat::NAME,
            MprisSetShuffle::NAME,
        ],
        dispatch,
    );
    follow_health(tasks, broker, cancel.clone(), Mpris::NAME, handle.health());
    follow(
        tasks,
        broker,
        cancel,
        handle.subscribe(),
        |broker, state| {
            publish(&broker, MprisPlayers::NAME, state);
        },
    );
}

pub fn notifications(
    tasks: &TaskTracker,
    broker: &Handle,
    cancel: CancellationToken,
    handle: NotificationsHandle,
) {
    let dispatch = one(tasks, handle.clone(), |handle, method, args| async move {
        match method.as_str() {
            NotificationsDismiss::NAME => {
                let NotificationsDismiss { id } = decode(args)?;
                handle.dismiss(id).await
            }
            NotificationsRemove::NAME => {
                let NotificationsRemove { id } = decode(args)?;
                handle.remove(id).await
            }
            NotificationsActivate::NAME => {
                let NotificationsActivate {
                    id,
                    activation_token,
                } = decode(args)?;
                handle.activate(id, activation_token).await
            }
            NotificationsInvokeAction::NAME => {
                let NotificationsInvokeAction {
                    id,
                    action,
                    activation_token,
                } = decode(args)?;
                handle.invoke_action(id, action, activation_token).await
            }
            NotificationsClearApp::NAME => {
                let NotificationsClearApp { app_id } = decode(args)?;
                handle.clear_app(app_id).await
            }
            NotificationsClearAll::NAME => {
                let _: NotificationsClearAll = decode(args)?;
                handle.clear_all().await
            }
            NotificationsSetDnd::NAME => {
                let NotificationsSetDnd { dnd } = decode(args)?;
                handle.set_dnd(dnd).await
            }
            _ => Err(unknown(Notifications::NAME, &method)),
        }
    });
    declare(
        broker,
        Notifications::NAME,
        &[NotificationsList::NAME, NotificationsDnd::NAME],
        &[
            NotificationsDismiss::NAME,
            NotificationsRemove::NAME,
            NotificationsActivate::NAME,
            NotificationsInvokeAction::NAME,
            NotificationsClearApp::NAME,
            NotificationsClearAll::NAME,
            NotificationsSetDnd::NAME,
        ],
        dispatch,
    );
    follow_health(
        tasks,
        broker,
        cancel.clone(),
        Notifications::NAME,
        handle.health(),
    );
    follow(
        tasks,
        broker,
        cancel,
        handle.subscribe(),
        publish_notifications,
    );
}

pub fn session(
    tasks: &TaskTracker,
    broker: &Handle,
    cancel: CancellationToken,
    handle: SessionHandle,
) {
    declare(
        broker,
        Session::NAME,
        &[SessionStatus::NAME],
        &[],
        Box::new(|_, _, _| {}),
    );
    follow_health(
        tasks,
        broker,
        cancel.clone(),
        Session::NAME,
        handle.health(),
    );
    follow(
        tasks,
        broker,
        cancel,
        handle.subscribe(),
        |broker, state| {
            if let Some(state) = state {
                publish(&broker, SessionStatus::NAME, state);
            }
        },
    );
}

fn publish_compositor(broker: Handle, state: CompositorState) {
    if let Some(value) = state.status {
        publish(&broker, CompositorStatus::NAME, value);
    }
    if let Some(value) = state.workspaces {
        publish(&broker, CompositorWorkspaces::NAME, value);
    }
    if let Some(value) = state.windows {
        publish(&broker, CompositorWindows::NAME, value);
    }
    if let Some(value) = state.outputs {
        publish(&broker, CompositorOutputs::NAME, value);
    }
    if let Some(value) = state.privacy {
        publish(&broker, CompositorPrivacy::NAME, value);
    }
}

fn publish_notifications(broker: Handle, state: NotificationsState) {
    if let Some(value) = state.list {
        publish(&broker, NotificationsList::NAME, value);
    }
    if let Some(value) = state.dnd {
        publish(&broker, NotificationsDnd::NAME, value);
    }
}

fn one<H, F, Fut>(tasks: &TaskTracker, handle: H, call: F) -> Dispatch
where
    H: Clone + Send + 'static,
    F: Fn(H, String, Value) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), CommandError>> + Send + 'static,
{
    let tasks = tasks.clone();
    Box::new(move |method, args, responder| {
        let future = call(handle.clone(), method.to_owned(), args);
        answer(&tasks, responder, future);
    })
}

fn answer<T, F>(tasks: &TaskTracker, responder: Responder, future: F)
where
    T: Serialize + Send + 'static,
    F: Future<Output = Result<T, CommandError>> + Send + 'static,
{
    tasks.spawn(async move {
        match future.await {
            Ok(value) => responder.ok(value),
            Err(error) => responder.fail(to_call_error(error)),
        }
    });
}

fn declare(
    broker: &Handle,
    service: &'static str,
    topics: &'static [&'static str],
    methods: &'static [&'static str],
    dispatch: Dispatch,
) {
    broker.send(Message::Declare {
        service,
        topics,
        methods,
        dispatch,
    });
}

fn follow<T, F>(
    tasks: &TaskTracker,
    broker: &Handle,
    cancel: CancellationToken,
    mut receiver: watch::Receiver<T>,
    publish: F,
) where
    T: Clone + Send + Sync + 'static,
    F: Fn(Handle, T) + Send + Sync + 'static,
{
    let initial = receiver.borrow_and_update().clone();
    publish(broker.clone(), initial);
    let broker = broker.clone();
    tasks.spawn(async move {
        loop {
            tokio::select! {
                () = cancel.cancelled() => break,
                changed = receiver.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    publish(broker.clone(), receiver.borrow_and_update().clone());
                }
            }
        }
    });
}

fn follow_health(
    tasks: &TaskTracker,
    broker: &Handle,
    cancel: CancellationToken,
    service: &'static str,
    receiver: watch::Receiver<glimpse_services::ServiceState>,
) {
    follow(tasks, broker, cancel, receiver, move |broker, state| {
        broker.report_health(service, state);
    });
}

fn publish<T: Serialize>(broker: &Handle, topic: &'static str, value: T) {
    match serde_json::to_value(value) {
        Ok(data) => broker.publish(topic, data),
        Err(error) => tracing::error!(topic, %error, "legacy payload failed to serialize"),
    }
}

fn decode<T: DeserializeOwned>(args: Value) -> Result<T, CommandError> {
    serde_json::from_value(args).map_err(|error| CommandError::InvalidArgument(error.to_string()))
}

fn unknown(service: &str, method: &str) -> CommandError {
    CommandError::InvalidArgument(format!("`{service}` does not answer `{method}`"))
}

fn to_call_error(error: CommandError) -> CallError {
    let code = match error {
        CommandError::InvalidArgument(_) => ErrorCode::InvalidArgs,
        CommandError::Unavailable(_) => ErrorCode::Unavailable,
        CommandError::Unsupported(_) => ErrorCode::Unsupported,
        CommandError::LimitExceeded(_) => ErrorCode::LimitExceeded,
        CommandError::Internal(_) => ErrorCode::Internal,
    };
    CallError::new(code, error.to_string())
}
