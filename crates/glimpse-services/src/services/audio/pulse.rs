use std::collections::HashMap;
use std::thread;
use std::time::{Duration, Instant};

use futures_util::{Stream, stream};
use libpulse_binding::callbacks::ListResult;
use libpulse_binding::context::introspect::{
    ClientInfo, SinkInfo, SinkInputInfo, SourceInfo, SourceOutputInfo,
};
use libpulse_binding::context::subscribe::InterestMaskSet;
use libpulse_binding::context::{Context, FlagSet as ContextFlagSet, State as ContextState};
use libpulse_binding::mainloop::threaded::Mainloop;
use libpulse_binding::operation::{Operation, State as OperationState};
use libpulse_binding::proplist::{Proplist, properties};
use libpulse_binding::volume::{ChannelVolumes, Volume};
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

use super::identify::{self, Stream as StreamInfo};
use super::model::{App, AudioError, Device, DeviceId, Direction, NAME_CAP};

const DEBOUNCE: Duration = Duration::from_millis(75);
const MAX_DEBOUNCE: Duration = Duration::from_millis(300);
const OP_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_CAPACITY: usize = 32;
const EVENTS_CAPACITY: usize = 32;
const SIGNAL_CAPACITY: usize = 8;
const APPLICATION_NAME: &str = "glimpse";
const PROP_KEY_CAP: usize = 256;
const PROP_VALUE_CAP: usize = 4096;

pub struct Snapshot {
    pub outputs: Vec<Device>,
    pub inputs: Vec<Device>,
    pub apps: Vec<App>,
}

pub enum Event {
    Ready(Client),
    Snapshot(Box<Snapshot>),
    Gone(String),
}

pub type Reply = oneshot::Sender<Result<(), AudioError>>;

pub enum Request {
    Refresh {
        reply: Reply,
    },
    SetDeviceVolume {
        dir: Direction,
        index: u32,
        percent: u32,
        reply: Reply,
    },
    SetDeviceMuted {
        dir: Direction,
        index: u32,
        muted: bool,
        reply: Reply,
    },
    SetDefault {
        dir: Direction,
        name: String,
        reply: Reply,
    },
    SetStreamVolume {
        dir: Direction,
        streams: Vec<(u32, u32)>,
        reply: Reply,
    },
    SetStreamMuted {
        dir: Direction,
        streams: Vec<u32>,
        muted: bool,
        reply: Reply,
    },
    MoveStreams {
        dir: Direction,
        streams: Vec<u32>,
        target: u32,
        reply: Reply,
    },
}

#[derive(Clone)]
pub struct Client {
    tx: mpsc::Sender<Request>,
}

impl Client {
    pub async fn send(&self, make: impl FnOnce(Reply) -> Request) -> Result<(), AudioError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let request = make(reply_tx);
        self.tx
            .send(request)
            .await
            .map_err(|_| AudioError::Unavailable)?;
        reply_rx.await.map_err(|_| AudioError::Unavailable)?
    }
}

pub async fn connect() -> impl Stream<Item = Event> + Send + 'static {
    let (request_tx, request_rx) = mpsc::channel(REQUEST_CAPACITY);
    let (events_tx, events_rx) = mpsc::channel(EVENTS_CAPACITY);
    let report_tx = events_tx.clone();

    if let Err(error) = thread::Builder::new()
        .name("glimpse-pulse".to_owned())
        .spawn(move || bridge_main(request_rx, request_tx, events_tx))
    {
        let _ = report_tx
            .send(Event::Gone(format!(
                "could not start the pulse bridge thread: {error}"
            )))
            .await;
    }

    stream::unfold(events_rx, |mut rx| async move {
        rx.recv().await.map(|event| (event, rx))
    })
}

fn bridge_main(
    request_rx: mpsc::Receiver<Request>,
    request_tx: mpsc::Sender<Request>,
    events_tx: mpsc::Sender<Event>,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = events_tx.blocking_send(Event::Gone(format!(
                "could not start the pulse bridge runtime: {error}"
            )));
            return;
        }
    };
    runtime.block_on(run(request_rx, request_tx, events_tx));
}

#[must_use]
struct MainloopGuard<'a> {
    mainloop: &'a mut Mainloop,
}

impl<'a> MainloopGuard<'a> {
    fn new(mainloop: &'a mut Mainloop) -> Self {
        mainloop.lock();
        Self { mainloop }
    }
}

impl Drop for MainloopGuard<'_> {
    fn drop(&mut self) {
        self.mainloop.unlock();
    }
}

async fn run(
    mut request_rx: mpsc::Receiver<Request>,
    request_tx: mpsc::Sender<Request>,
    events_tx: mpsc::Sender<Event>,
) {
    let Some(mut mainloop) = Mainloop::new() else {
        let _ = events_tx
            .send(Event::Gone(
                "could not create the pulse mainloop".to_owned(),
            ))
            .await;
        return;
    };

    let Some(mut proplist) = Proplist::new() else {
        let _ = events_tx
            .send(Event::Gone(
                "could not create a pulse property list".to_owned(),
            ))
            .await;
        return;
    };
    let _ = proplist.set_str(properties::APPLICATION_NAME, APPLICATION_NAME);

    let Some(mut context) = Context::new_with_proplist(&mainloop, APPLICATION_NAME, &proplist)
    else {
        let _ = events_tx
            .send(Event::Gone("could not create the pulse context".to_owned()))
            .await;
        return;
    };

    let (signal_tx, mut signal_rx) = mpsc::channel::<()>(SIGNAL_CAPACITY);

    let connected = {
        let _guard = MainloopGuard::new(&mut mainloop);
        let cb_tx = signal_tx.clone();
        context.set_state_callback(Some(Box::new(move || {
            let _ = cb_tx.try_send(());
        })));
        context.connect(None, ContextFlagSet::NOFLAGS, None)
    };
    if let Err(error) = connected {
        let _ = events_tx
            .send(Event::Gone(format!(
                "could not connect to the pulse server: {error}"
            )))
            .await;
        return;
    }

    if let Err(error) = mainloop.start() {
        let _ = events_tx
            .send(Event::Gone(format!(
                "could not start the pulse mainloop: {error}"
            )))
            .await;
        return;
    }

    loop {
        let state = {
            let _guard = MainloopGuard::new(&mut mainloop);
            context.get_state()
        };

        match state {
            ContextState::Ready => break,
            ContextState::Failed | ContextState::Terminated => {
                let _ = events_tx
                    .send(Event::Gone("the pulse connection failed".to_owned()))
                    .await;
                {
                    let _guard = MainloopGuard::new(&mut mainloop);
                    context.disconnect();
                }
                mainloop.stop();
                return;
            }
            _ => {
                signal_rx.recv().await;
            }
        }
    }

    {
        let _guard = MainloopGuard::new(&mut mainloop);
        let cb_tx = signal_tx.clone();
        context.set_subscribe_callback(Some(Box::new(move |_facility, _operation, _index| {
            let _ = cb_tx.try_send(());
        })));
    }

    let subscribed = run_bool_op(&mut mainloop, |cb| {
        context.subscribe(
            InterestMaskSet::SINK
                | InterestMaskSet::SOURCE
                | InterestMaskSet::SINK_INPUT
                | InterestMaskSet::SOURCE_OUTPUT
                | InterestMaskSet::SERVER,
            cb,
        )
    })
    .await;

    if let Err(error) = subscribed {
        let _ = events_tx
            .send(Event::Gone(format!(
                "could not subscribe to pulse change notifications: {error}"
            )))
            .await;
        shutdown(&mut mainloop, &mut context);
        return;
    }

    if events_tx
        .send(Event::Ready(Client { tx: request_tx }))
        .await
        .is_err()
    {
        shutdown(&mut mainloop, &mut context);
        return;
    }

    match refresh(&mut mainloop, &mut context).await {
        Ok(snapshot) => {
            if events_tx
                .send(Event::Snapshot(Box::new(snapshot)))
                .await
                .is_err()
            {
                shutdown(&mut mainloop, &mut context);
                return;
            }
        }
        Err(reason) => {
            let _ = events_tx.send(Event::Gone(reason)).await;
            shutdown(&mut mainloop, &mut context);
            return;
        }
    }

    'pump: loop {
        tokio::select! {
            request = request_rx.recv() => {
                let Some(request) = request else { break 'pump; };
                if !dispatch(&mut mainloop, &mut context, &events_tx, request).await {
                    break 'pump;
                }
            }
            _ = signal_rx.recv() => {
                let deadline = Instant::now() + MAX_DEBOUNCE;
                loop {
                    match timeout(DEBOUNCE, signal_rx.recv()).await {
                        Ok(Some(())) if Instant::now() < deadline => continue,
                        _ => break,
                    }
                }

                let state = {
                    let _guard = MainloopGuard::new(&mut mainloop);
                    context.get_state()
                };

                if state != ContextState::Ready {
                    let _ = events_tx.send(Event::Gone("the pulse connection was lost".to_owned())).await;
                    break 'pump;
                }

                match refresh(&mut mainloop, &mut context).await {
                    Ok(snapshot) => {
                        if events_tx.send(Event::Snapshot(Box::new(snapshot))).await.is_err() {
                            break 'pump;
                        }
                    }
                    Err(reason) => {
                        let _ = events_tx.send(Event::Gone(reason)).await;
                        break 'pump;
                    }
                }
            }
        }
    }

    shutdown(&mut mainloop, &mut context);
}

fn shutdown(mainloop: &mut Mainloop, context: &mut Context) {
    {
        let _guard = MainloopGuard::new(mainloop);
        context.set_state_callback(None);
        context.set_subscribe_callback(None);
        context.disconnect();
    }
    mainloop.stop();
}

async fn dispatch(
    mainloop: &mut Mainloop,
    context: &mut Context,
    events_tx: &mpsc::Sender<Event>,
    request: Request,
) -> bool {
    match request {
        Request::Refresh { reply } => finish_refresh(mainloop, context, events_tx, reply).await,
        Request::SetDeviceVolume {
            dir,
            index,
            percent,
            reply,
        } => {
            let result = set_device_volume(mainloop, context, dir, index, percent).await;
            finish_command(mainloop, context, events_tx, reply, result).await
        }
        Request::SetDeviceMuted {
            dir,
            index,
            muted,
            reply,
        } => {
            let result = set_device_muted(mainloop, context, dir, index, muted).await;
            finish_command(mainloop, context, events_tx, reply, result).await
        }
        Request::SetDefault { dir, name, reply } => {
            let result = set_default(mainloop, context, dir, &name).await;
            finish_command(mainloop, context, events_tx, reply, result).await
        }
        Request::SetStreamVolume {
            dir,
            streams,
            reply,
        } => {
            let result = set_stream_volume(mainloop, context, dir, streams).await;
            finish_command(mainloop, context, events_tx, reply, result).await
        }
        Request::SetStreamMuted {
            dir,
            streams,
            muted,
            reply,
        } => {
            let result = set_stream_muted(mainloop, context, dir, streams, muted).await;
            finish_command(mainloop, context, events_tx, reply, result).await
        }
        Request::MoveStreams {
            dir,
            streams,
            target,
            reply,
        } => {
            let result = move_streams(mainloop, context, dir, streams, target).await;
            finish_command(mainloop, context, events_tx, reply, result).await
        }
    }
}

async fn finish_refresh(
    mainloop: &mut Mainloop,
    context: &mut Context,
    events_tx: &mpsc::Sender<Event>,
    reply: Reply,
) -> bool {
    match refresh(mainloop, context).await {
        Ok(snapshot) => {
            let _ = reply.send(Ok(()));
            events_tx
                .send(Event::Snapshot(Box::new(snapshot)))
                .await
                .is_ok()
        }
        Err(reason) => {
            let _ = reply.send(Err(AudioError::Unavailable));
            let _ = events_tx.send(Event::Gone(reason)).await;
            false
        }
    }
}

async fn finish_command(
    mainloop: &mut Mainloop,
    context: &mut Context,
    events_tx: &mpsc::Sender<Event>,
    reply: Reply,
    result: Result<(), AudioError>,
) -> bool {
    let connection_lost = matches!(result, Err(AudioError::Unavailable)) && {
        let _guard = MainloopGuard::new(mainloop);
        context.get_state() != ContextState::Ready
    };

    let _ = reply.send(result);

    if connection_lost {
        let _ = events_tx
            .send(Event::Gone("the pulse connection was lost".to_owned()))
            .await;
        return false;
    }
    true
}

struct RawDevice {
    index: u32,
    pa_name: String,
    label: String,
    icon_name: Option<String>,
    form_factor: Option<String>,
    volume: u32,
    muted: bool,
    is_monitor: bool,
}

struct RawStream {
    index: u32,
    client: Option<u32>,
    device_index: u32,
    volume: u32,
    muted: bool,
    adjustable: bool,
    corked: bool,
    props: HashMap<String, String>,
}

struct RawServerInfo {
    default_sink_name: Option<String>,
    default_source_name: Option<String>,
}

async fn refresh(mainloop: &mut Mainloop, context: &mut Context) -> Result<Snapshot, String> {
    let server = get_server_info(mainloop, context).await?;
    let raw_sinks = get_sink_list(mainloop, context).await?;
    let raw_sources = get_source_list(mainloop, context).await?;
    let raw_sink_inputs = get_sink_input_list(mainloop, context).await?;
    let raw_source_outputs = get_source_output_list(mainloop, context).await?;
    let clients = get_client_map(mainloop, context).await?;

    let sink_names: HashMap<u32, String> = raw_sinks
        .iter()
        .map(|raw| (raw.index, raw.pa_name.clone()))
        .collect();
    let source_names: HashMap<u32, String> = raw_sources
        .iter()
        .map(|raw| (raw.index, raw.pa_name.clone()))
        .collect();

    let outputs = devices_from_raw(raw_sinks, server.default_sink_name.as_deref());
    let inputs = devices_from_raw(raw_sources, server.default_source_name.as_deref());

    let playback: Vec<StreamInfo> = raw_sink_inputs
        .into_iter()
        .map(|raw| finish_stream(raw, &sink_names, &clients))
        .collect();
    let capture: Vec<StreamInfo> = raw_source_outputs
        .into_iter()
        .map(|raw| finish_stream(raw, &source_names, &clients))
        .collect();

    let apps = identify::group(playback, capture);

    Ok(Snapshot {
        outputs,
        inputs,
        apps,
    })
}

fn devices_from_raw(raw: Vec<RawDevice>, default_name: Option<&str>) -> Vec<Device> {
    raw.into_iter()
        .filter(|device| !device.is_monitor)
        .map(|device| finish_device(device, default_name))
        .collect()
}

fn finish_device(raw: RawDevice, default_name: Option<&str>) -> Device {
    let default = default_name == Some(raw.pa_name.as_str());
    Device {
        id: DeviceId::new(raw.pa_name),
        index: raw.index,
        name: raw.label,
        icon_name: raw.icon_name,
        form_factor: raw.form_factor,
        volume: raw.volume,
        muted: raw.muted,
        default,
    }
}

fn finish_stream(
    raw: RawStream,
    devices: &HashMap<u32, String>,
    clients: &HashMap<u32, HashMap<String, String>>,
) -> StreamInfo {
    StreamInfo {
        index: raw.index,
        client: raw.client,
        client_props: raw.client.and_then(|id| clients.get(&id).cloned()),
        device: DeviceId::new(devices.get(&raw.device_index).cloned().unwrap_or_default()),
        volume: raw.volume,
        muted: raw.muted,
        adjustable: raw.adjustable,
        corked: raw.corked,
        props: raw.props,
    }
}

fn sink_to_raw(info: &SinkInfo) -> RawDevice {
    let name = info.name.as_deref().unwrap_or_default().to_owned();
    let label = info
        .description
        .as_deref()
        .map(cap)
        .unwrap_or_else(|| cap(&name));
    RawDevice {
        index: info.index,
        pa_name: name,
        label,
        icon_name: proplist_str(&info.proplist, properties::DEVICE_ICON_NAME),
        form_factor: proplist_str(&info.proplist, properties::DEVICE_FORM_FACTOR),
        volume: volume_to_percent(info.volume.max()),
        muted: info.mute,
        is_monitor: false,
    }
}

fn source_to_raw(info: &SourceInfo) -> RawDevice {
    let name = info.name.as_deref().unwrap_or_default().to_owned();
    let label = info
        .description
        .as_deref()
        .map(cap)
        .unwrap_or_else(|| cap(&name));
    RawDevice {
        index: info.index,
        pa_name: name,
        label,
        icon_name: proplist_str(&info.proplist, properties::DEVICE_ICON_NAME),
        form_factor: proplist_str(&info.proplist, properties::DEVICE_FORM_FACTOR),
        volume: volume_to_percent(info.volume.max()),
        muted: info.mute,
        is_monitor: info.monitor_of_sink.is_some(),
    }
}

fn sink_input_to_raw(info: &SinkInputInfo) -> RawStream {
    RawStream {
        index: info.index,
        client: info.client,
        device_index: info.sink,
        volume: volume_to_percent(info.volume.max()),
        muted: info.mute,
        adjustable: info.has_volume && info.volume_writable,
        corked: info.corked,
        props: proplist_to_map(&info.proplist),
    }
}

fn source_output_to_raw(info: &SourceOutputInfo) -> RawStream {
    RawStream {
        index: info.index,
        client: info.client,
        device_index: info.source,
        volume: volume_to_percent(info.volume.max()),
        muted: info.mute,
        adjustable: info.has_volume && info.volume_writable,
        corked: info.corked,
        props: proplist_to_map(&info.proplist),
    }
}

async fn timed_recv<T, C: ?Sized>(
    mainloop: &mut Mainloop,
    mut op: Operation<C>,
    rx: oneshot::Receiver<T>,
) -> Result<T, ()> {
    match timeout(OP_TIMEOUT, rx).await {
        Ok(Ok(value)) => {
            let _guard = MainloopGuard::new(mainloop);
            drop(op);
            Ok(value)
        }
        Ok(Err(_)) => {
            let _guard = MainloopGuard::new(mainloop);
            drop(op);
            Err(())
        }
        Err(_) => {
            let _guard = MainloopGuard::new(mainloop);
            if op.get_state() == OperationState::Running {
                op.cancel();
            }
            drop(op);
            Err(())
        }
    }
}

async fn recv_list<T, C: ?Sized>(
    mainloop: &mut Mainloop,
    op: Operation<C>,
    rx: oneshot::Receiver<Result<Vec<T>, ()>>,
    what: &str,
) -> Result<Vec<T>, String> {
    match timed_recv(mainloop, op, rx).await {
        Ok(Ok(items)) => Ok(items),
        Ok(Err(())) => Err(format!("the pulse server reported an error listing {what}")),
        Err(()) => Err(format!(
            "the pulse server did not answer a {what} list request"
        )),
    }
}

async fn get_server_info(
    mainloop: &mut Mainloop,
    context: &mut Context,
) -> Result<RawServerInfo, String> {
    let (tx, rx) = oneshot::channel::<RawServerInfo>();
    let mut tx = Some(tx);

    let op = {
        let _guard = MainloopGuard::new(mainloop);
        let introspector = context.introspect();
        let op = introspector.get_server_info(move |info| {
            if let Some(tx) = tx.take() {
                let _ = tx.send(RawServerInfo {
                    default_sink_name: info.default_sink_name.as_deref().map(str::to_owned),
                    default_source_name: info.default_source_name.as_deref().map(str::to_owned),
                });
            }
        });
        drop(introspector);
        op
    };

    timed_recv(mainloop, op, rx)
        .await
        .map_err(|()| "the pulse server did not answer a server-info request".to_owned())
}

async fn get_sink_list(
    mainloop: &mut Mainloop,
    context: &mut Context,
) -> Result<Vec<RawDevice>, String> {
    let (tx, rx) = oneshot::channel::<Result<Vec<RawDevice>, ()>>();
    let mut tx = Some(tx);
    let mut items = Vec::new();

    let op = {
        let _guard = MainloopGuard::new(mainloop);
        let introspector = context.introspect();
        let op =
            introspector.get_sink_info_list(move |result: ListResult<&SinkInfo>| match result {
                ListResult::Item(info) => items.push(sink_to_raw(info)),
                ListResult::End => {
                    if let Some(tx) = tx.take() {
                        let _ = tx.send(Ok(std::mem::take(&mut items)));
                    }
                }
                ListResult::Error => {
                    if let Some(tx) = tx.take() {
                        let _ = tx.send(Err(()));
                    }
                }
            });
        drop(introspector);
        op
    };

    recv_list(mainloop, op, rx, "sink").await
}

async fn get_source_list(
    mainloop: &mut Mainloop,
    context: &mut Context,
) -> Result<Vec<RawDevice>, String> {
    let (tx, rx) = oneshot::channel::<Result<Vec<RawDevice>, ()>>();
    let mut tx = Some(tx);
    let mut items = Vec::new();

    let op = {
        let _guard = MainloopGuard::new(mainloop);
        let introspector = context.introspect();
        let op =
            introspector.get_source_info_list(
                move |result: ListResult<&SourceInfo>| match result {
                    ListResult::Item(info) => items.push(source_to_raw(info)),
                    ListResult::End => {
                        if let Some(tx) = tx.take() {
                            let _ = tx.send(Ok(std::mem::take(&mut items)));
                        }
                    }
                    ListResult::Error => {
                        if let Some(tx) = tx.take() {
                            let _ = tx.send(Err(()));
                        }
                    }
                },
            );
        drop(introspector);
        op
    };

    recv_list(mainloop, op, rx, "source").await
}

async fn get_sink_input_list(
    mainloop: &mut Mainloop,
    context: &mut Context,
) -> Result<Vec<RawStream>, String> {
    let (tx, rx) = oneshot::channel::<Result<Vec<RawStream>, ()>>();
    let mut tx = Some(tx);
    let mut items = Vec::new();

    let op = {
        let _guard = MainloopGuard::new(mainloop);
        let introspector = context.introspect();
        let op =
            introspector.get_sink_input_info_list(move |result: ListResult<&SinkInputInfo>| {
                match result {
                    ListResult::Item(info) => items.push(sink_input_to_raw(info)),
                    ListResult::End => {
                        if let Some(tx) = tx.take() {
                            let _ = tx.send(Ok(std::mem::take(&mut items)));
                        }
                    }
                    ListResult::Error => {
                        if let Some(tx) = tx.take() {
                            let _ = tx.send(Err(()));
                        }
                    }
                }
            });
        drop(introspector);
        op
    };

    recv_list(mainloop, op, rx, "sink input").await
}

async fn get_source_output_list(
    mainloop: &mut Mainloop,
    context: &mut Context,
) -> Result<Vec<RawStream>, String> {
    let (tx, rx) = oneshot::channel::<Result<Vec<RawStream>, ()>>();
    let mut tx = Some(tx);
    let mut items = Vec::new();

    let op = {
        let _guard = MainloopGuard::new(mainloop);
        let introspector = context.introspect();
        let op = introspector.get_source_output_info_list(
            move |result: ListResult<&SourceOutputInfo>| match result {
                ListResult::Item(info) => items.push(source_output_to_raw(info)),
                ListResult::End => {
                    if let Some(tx) = tx.take() {
                        let _ = tx.send(Ok(std::mem::take(&mut items)));
                    }
                }
                ListResult::Error => {
                    if let Some(tx) = tx.take() {
                        let _ = tx.send(Err(()));
                    }
                }
            },
        );
        drop(introspector);
        op
    };

    recv_list(mainloop, op, rx, "source output").await
}

async fn get_client_map(
    mainloop: &mut Mainloop,
    context: &mut Context,
) -> Result<HashMap<u32, HashMap<String, String>>, String> {
    let (tx, rx) = oneshot::channel::<Result<Vec<(u32, HashMap<String, String>)>, ()>>();
    let mut tx = Some(tx);
    let mut items = Vec::new();

    let op = {
        let _guard = MainloopGuard::new(mainloop);
        let introspector = context.introspect();
        let op =
            introspector.get_client_info_list(
                move |result: ListResult<&ClientInfo>| match result {
                    ListResult::Item(info) => {
                        items.push((info.index, proplist_to_map(&info.proplist)))
                    }
                    ListResult::End => {
                        if let Some(tx) = tx.take() {
                            let _ = tx.send(Ok(std::mem::take(&mut items)));
                        }
                    }
                    ListResult::Error => {
                        if let Some(tx) = tx.take() {
                            let _ = tx.send(Err(()));
                        }
                    }
                },
            );
        drop(introspector);
        op
    };

    recv_list(mainloop, op, rx, "client")
        .await
        .map(|items| items.into_iter().collect())
}

async fn get_sink_channel_volumes(
    mainloop: &mut Mainloop,
    context: &mut Context,
    index: u32,
) -> Result<ChannelVolumes, AudioError> {
    let (tx, rx) = oneshot::channel::<Option<ChannelVolumes>>();
    let mut tx = Some(tx);

    let op = {
        let _guard = MainloopGuard::new(mainloop);
        let introspector = context.introspect();
        let op =
            introspector.get_sink_info_by_index(index, move |result: ListResult<&SinkInfo>| {
                match result {
                    ListResult::Item(info) => {
                        if let Some(tx) = tx.take() {
                            let _ = tx.send(Some(info.volume));
                        }
                    }
                    ListResult::End | ListResult::Error => {
                        if let Some(tx) = tx.take() {
                            let _ = tx.send(None);
                        }
                    }
                }
            });
        drop(introspector);
        op
    };

    match timed_recv(mainloop, op, rx).await {
        Ok(Some(volume)) => Ok(volume),
        Ok(None) | Err(()) => Err(AudioError::Unavailable),
    }
}

async fn get_source_channel_volumes(
    mainloop: &mut Mainloop,
    context: &mut Context,
    index: u32,
) -> Result<ChannelVolumes, AudioError> {
    let (tx, rx) = oneshot::channel::<Option<ChannelVolumes>>();
    let mut tx = Some(tx);

    let op = {
        let _guard = MainloopGuard::new(mainloop);
        let introspector = context.introspect();
        let op =
            introspector.get_source_info_by_index(index, move |result: ListResult<&SourceInfo>| {
                match result {
                    ListResult::Item(info) => {
                        if let Some(tx) = tx.take() {
                            let _ = tx.send(Some(info.volume));
                        }
                    }
                    ListResult::End | ListResult::Error => {
                        if let Some(tx) = tx.take() {
                            let _ = tx.send(None);
                        }
                    }
                }
            });
        drop(introspector);
        op
    };

    match timed_recv(mainloop, op, rx).await {
        Ok(Some(volume)) => Ok(volume),
        Ok(None) | Err(()) => Err(AudioError::Unavailable),
    }
}

async fn get_sink_input_channel_volumes(
    mainloop: &mut Mainloop,
    context: &mut Context,
    index: u32,
) -> Result<ChannelVolumes, AudioError> {
    let (tx, rx) = oneshot::channel::<Option<ChannelVolumes>>();
    let mut tx = Some(tx);

    let op = {
        let _guard = MainloopGuard::new(mainloop);
        let introspector = context.introspect();
        let op =
            introspector.get_sink_input_info(index, move |result: ListResult<&SinkInputInfo>| {
                match result {
                    ListResult::Item(info) => {
                        if let Some(tx) = tx.take() {
                            let _ = tx.send(Some(info.volume));
                        }
                    }
                    ListResult::End | ListResult::Error => {
                        if let Some(tx) = tx.take() {
                            let _ = tx.send(None);
                        }
                    }
                }
            });
        drop(introspector);
        op
    };

    match timed_recv(mainloop, op, rx).await {
        Ok(Some(volume)) => Ok(volume),
        Ok(None) | Err(()) => Err(AudioError::Unavailable),
    }
}

async fn get_source_output_channel_volumes(
    mainloop: &mut Mainloop,
    context: &mut Context,
    index: u32,
) -> Result<ChannelVolumes, AudioError> {
    let (tx, rx) = oneshot::channel::<Option<ChannelVolumes>>();
    let mut tx = Some(tx);

    let op = {
        let _guard = MainloopGuard::new(mainloop);
        let introspector = context.introspect();
        let op = introspector.get_source_output_info(
            index,
            move |result: ListResult<&SourceOutputInfo>| match result {
                ListResult::Item(info) => {
                    if let Some(tx) = tx.take() {
                        let _ = tx.send(Some(info.volume));
                    }
                }
                ListResult::End | ListResult::Error => {
                    if let Some(tx) = tx.take() {
                        let _ = tx.send(None);
                    }
                }
            },
        );
        drop(introspector);
        op
    };

    match timed_recv(mainloop, op, rx).await {
        Ok(Some(volume)) => Ok(volume),
        Ok(None) | Err(()) => Err(AudioError::Unavailable),
    }
}

async fn run_bool_op(
    mainloop: &mut Mainloop,
    make: impl FnOnce(Box<dyn FnMut(bool)>) -> Operation<dyn FnMut(bool)>,
) -> Result<(), AudioError> {
    let (tx, rx) = oneshot::channel::<bool>();
    let mut tx = Some(tx);
    let callback: Box<dyn FnMut(bool)> = Box::new(move |success| {
        if let Some(tx) = tx.take() {
            let _ = tx.send(success);
        }
    });

    let op = {
        let _guard = MainloopGuard::new(mainloop);
        make(callback)
    };

    match timed_recv(mainloop, op, rx).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(AudioError::Refused(
            "the pulse server refused the command".to_owned(),
        )),
        Err(()) => Err(AudioError::Unavailable),
    }
}

async fn set_device_volume(
    mainloop: &mut Mainloop,
    context: &mut Context,
    dir: Direction,
    index: u32,
    percent: u32,
) -> Result<(), AudioError> {
    let mut volume = match dir {
        Direction::Output => get_sink_channel_volumes(mainloop, context, index).await?,
        Direction::Input => get_source_channel_volumes(mainloop, context, index).await?,
    };
    if volume.scale(percent_to_volume(percent)).is_none() {
        return Err(AudioError::Refused(
            "the requested volume could not be applied".to_owned(),
        ));
    }

    run_bool_op(mainloop, |cb| {
        let mut introspector = context.introspect();
        match dir {
            Direction::Output => introspector.set_sink_volume_by_index(index, &volume, Some(cb)),
            Direction::Input => introspector.set_source_volume_by_index(index, &volume, Some(cb)),
        }
    })
    .await
}

async fn set_device_muted(
    mainloop: &mut Mainloop,
    context: &mut Context,
    dir: Direction,
    index: u32,
    muted: bool,
) -> Result<(), AudioError> {
    run_bool_op(mainloop, |cb| {
        let mut introspector = context.introspect();
        match dir {
            Direction::Output => introspector.set_sink_mute_by_index(index, muted, Some(cb)),
            Direction::Input => introspector.set_source_mute_by_index(index, muted, Some(cb)),
        }
    })
    .await
}

async fn set_default(
    mainloop: &mut Mainloop,
    context: &mut Context,
    dir: Direction,
    name: &str,
) -> Result<(), AudioError> {
    run_bool_op(mainloop, |cb| match dir {
        Direction::Output => context.set_default_sink(name, cb),
        Direction::Input => context.set_default_source(name, cb),
    })
    .await
}

async fn set_stream_volume(
    mainloop: &mut Mainloop,
    context: &mut Context,
    dir: Direction,
    streams: Vec<(u32, u32)>,
) -> Result<(), AudioError> {
    for (index, percent) in streams {
        let mut volume = match dir {
            Direction::Output => get_sink_input_channel_volumes(mainloop, context, index).await?,
            Direction::Input => get_source_output_channel_volumes(mainloop, context, index).await?,
        };
        if volume.scale(percent_to_volume(percent)).is_none() {
            return Err(AudioError::Refused(
                "the requested volume could not be applied".to_owned(),
            ));
        }

        run_bool_op(mainloop, |cb| {
            let mut introspector = context.introspect();
            match dir {
                Direction::Output => introspector.set_sink_input_volume(index, &volume, Some(cb)),
                Direction::Input => introspector.set_source_output_volume(index, &volume, Some(cb)),
            }
        })
        .await?;
    }
    Ok(())
}

async fn set_stream_muted(
    mainloop: &mut Mainloop,
    context: &mut Context,
    dir: Direction,
    streams: Vec<u32>,
    muted: bool,
) -> Result<(), AudioError> {
    for index in streams {
        run_bool_op(mainloop, |cb| {
            let mut introspector = context.introspect();
            match dir {
                Direction::Output => introspector.set_sink_input_mute(index, muted, Some(cb)),
                Direction::Input => introspector.set_source_output_mute(index, muted, Some(cb)),
            }
        })
        .await?;
    }
    Ok(())
}

async fn move_streams(
    mainloop: &mut Mainloop,
    context: &mut Context,
    dir: Direction,
    streams: Vec<u32>,
    target: u32,
) -> Result<(), AudioError> {
    for index in streams {
        run_bool_op(mainloop, |cb| {
            let mut introspector = context.introspect();
            match dir {
                Direction::Output => introspector.move_sink_input_by_index(index, target, Some(cb)),
                Direction::Input => {
                    introspector.move_source_output_by_index(index, target, Some(cb))
                }
            }
        })
        .await?;
    }
    Ok(())
}

fn proplist_str(proplist: &Proplist, key: &str) -> Option<String> {
    proplist.get_str(key).map(|value| cap(&value))
}

fn proplist_to_map(proplist: &Proplist) -> HashMap<String, String> {
    proplist
        .iter()
        .filter(|key| key.chars().count() <= PROP_KEY_CAP)
        .filter_map(|key| {
            let value = proplist.get_str(&key)?;
            Some((key, bound(&value)))
        })
        .collect()
}

fn bound(value: &str) -> String {
    value.chars().take(PROP_VALUE_CAP).collect()
}

fn volume_to_percent(volume: Volume) -> u32 {
    let normal = u64::from(Volume::NORMAL.0);
    ((u64::from(volume.0) * 100 + normal / 2) / normal) as u32
}

fn percent_to_volume(percent: u32) -> Volume {
    let normal = u64::from(Volume::NORMAL.0);
    let raw = (u64::from(percent) * normal) / 100;
    Volume(raw.min(u64::from(Volume::MAX.0)) as u32)
}

fn cap(value: &str) -> String {
    value.chars().take(NAME_CAP).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_round_trips_at_the_boundaries() {
        for percent in [0, 40, 50, 100] {
            let volume = percent_to_volume(percent);
            assert_eq!(volume_to_percent(volume), percent);
        }
    }

    #[test]
    fn volume_to_percent_rounds_rather_than_floors() {
        assert_eq!(volume_to_percent(Volume(55050)), 84);
        assert_eq!(volume_to_percent(Volume(13107)), 20);
        assert_eq!(volume_to_percent(Volume(26214)), 40);
    }

    #[test]
    fn percent_to_volume_clamps_instead_of_wrapping() {
        assert_eq!(percent_to_volume(u32::MAX), Volume::MAX);
    }

    #[test]
    fn scale_preserves_an_imbalanced_stereo_mix() {
        let mut volume = ChannelVolumes::default();
        volume.set_len(2);
        volume.get_mut()[0] = percent_to_volume(100);
        volume.get_mut()[1] = percent_to_volume(50);

        volume.scale(percent_to_volume(50)).expect("scale succeeds");

        assert_eq!(volume_to_percent(volume.get()[0]), 50);
        assert_eq!(volume_to_percent(volume.get()[1]), 25);
    }

    fn raw_device(index: u32, pa_name: &str, is_monitor: bool) -> RawDevice {
        RawDevice {
            index,
            pa_name: pa_name.to_owned(),
            label: pa_name.to_owned(),
            icon_name: None,
            form_factor: None,
            volume: 100,
            muted: false,
            is_monitor,
        }
    }

    #[test]
    fn devices_from_raw_flags_the_matching_default() {
        let raw = vec![
            raw_device(62, "alsa_output.pci-0000_66_00.6.analog-stereo", false),
            raw_device(2736, "bluez_output.F8_4E_17_BC_EE_D5.1", false),
        ];

        let devices = devices_from_raw(raw, Some("bluez_output.F8_4E_17_BC_EE_D5.1"));

        assert_eq!(devices.len(), 2);
        assert!(!devices[0].default);
        assert!(devices[1].default);
        assert_eq!(
            devices[1].id,
            DeviceId::new("bluez_output.F8_4E_17_BC_EE_D5.1")
        );
    }

    #[test]
    fn devices_from_raw_drops_a_source_monitoring_a_sink() {
        let raw = vec![
            raw_device(63, "alsa_input.pci-0000_66_00.6.analog-stereo", false),
            raw_device(
                62,
                "alsa_output.pci-0000_66_00.6.analog-stereo.monitor",
                true,
            ),
        ];

        let devices = devices_from_raw(raw, Some("alsa_input.pci-0000_66_00.6.analog-stereo"));

        assert_eq!(devices.len(), 1);
        assert_eq!(
            devices[0].id,
            DeviceId::new("alsa_input.pci-0000_66_00.6.analog-stereo")
        );
        assert!(devices[0].default);
    }

    #[test]
    fn devices_from_raw_does_not_cap_the_pa_name() {
        let long_name =
            "alsa_output.usb-Focusrite_Scarlett_Solo_USB_Y7DGS9J06E5D2A-00.analog-stereo";
        assert!(long_name.len() > NAME_CAP);
        let raw = vec![raw_device(1, long_name, false)];

        let devices = devices_from_raw(raw, Some(long_name));

        assert_eq!(devices[0].id, DeviceId::new(long_name));
        assert!(
            devices[0].default,
            "a capped pa_name would never match the (also capped) default"
        );
    }

    #[test]
    fn proplist_map_drops_an_oversized_key_rather_than_truncating_it() {
        let mut proplist = Proplist::new().expect("a proplist");
        let long_key = "x".repeat(PROP_KEY_CAP + 1);
        proplist
            .set_str(&long_key, "value")
            .expect("set the long key");
        proplist
            .set_str("short.key", "kept")
            .expect("set the short key");

        let map = proplist_to_map(&proplist);

        assert!(!map.contains_key(&long_key));
        assert_eq!(map.get("short.key").map(String::as_str), Some("kept"));
    }

    #[tokio::test]
    #[ignore = "needs a running pulse or pipewire-pulse server"]
    async fn a_live_snapshot_matches_pactl() {
        use std::process::Command;

        use futures_util::StreamExt;

        let sinks_output = Command::new("pactl")
            .args(["list", "short", "sinks"])
            .output()
            .expect("pactl is available");
        let expected_sink_count = String::from_utf8_lossy(&sinks_output.stdout)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();

        let default_output = Command::new("pactl")
            .args(["get-default-sink"])
            .output()
            .expect("pactl is available");
        let expected_default = String::from_utf8_lossy(&default_output.stdout)
            .trim()
            .to_owned();

        let mut events = Box::pin(connect().await);
        let mut client = None;
        let mut snapshot = None;
        while let Some(event) = events.next().await {
            match event {
                Event::Ready(c) => client = Some(c),
                Event::Snapshot(s) => {
                    snapshot = Some(s);
                    break;
                }
                Event::Gone(reason) => panic!("the pulse bridge reported Gone: {reason}"),
            }
        }
        let _ = client;
        let snapshot = snapshot.expect("a snapshot before the event stream ended");

        assert_eq!(
            snapshot.outputs.len(),
            expected_sink_count,
            "sink count disagrees with `pactl list short sinks`"
        );
        let default = snapshot
            .outputs
            .iter()
            .find(|device| device.default)
            .expect("a default sink flagged");
        assert_eq!(default.id, DeviceId::new(expected_default));
    }
}
