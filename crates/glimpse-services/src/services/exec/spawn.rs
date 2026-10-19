use std::collections::{BTreeMap, VecDeque};
use std::os::fd::OwnedFd;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use futures_util::stream::BoxStream;
use futures_util::{SinkExt, Stream, StreamExt, stream};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};
use tokio_util::sync::CancellationToken;

use super::launcher::{Launcher, UnitState, build_spec, panel_env};
use super::{Catalog, Event, FromApplet, Link, MAX_LINE, MAX_UPDATES_PER_SECOND, Outgoing, Tree};

pub const EXEC_MAX_HELLOS: usize = 5;
pub const EXEC_HELLO_WINDOW: Duration = Duration::from_secs(10);

pub(super) struct SourceArgs {
    pub slot: u64,
    pub spawn: u64,
    pub id: String,
    pub env: BTreeMap<String, String>,
    pub state_base: Option<PathBuf>,
    pub entry: Option<super::Entry>,
    pub cwd: Option<PathBuf>,
    pub restart: bool,
    pub stderr: Option<OwnedFd>,
    pub unit: String,
    pub socket: PathBuf,
    pub launcher: Arc<dyn Launcher>,
    pub stop: CancellationToken,
}

struct SlotStream {
    catalog: Arc<dyn Catalog>,
    slot: u64,
    spawn: u64,
    id: String,
    env: BTreeMap<String, String>,
    state_base: Option<PathBuf>,
    entry: Option<super::Entry>,
    cwd: Option<PathBuf>,
    restart: bool,
    stderr: Option<OwnedFd>,
    unit: String,
    socket: PathBuf,
    socket_id: Option<(u64, u64)>,
    launcher: Arc<dyn Launcher>,
    stop: CancellationToken,
    connection: CancellationToken,
    hello_state: String,
    resolved: Option<super::Entry>,
    catalog_refused: bool,
    listener: Option<UnixListener>,
    watch: Option<BoxStream<'static, UnitState>>,
    reader: Option<FramedRead<tokio::io::ReadHalf<UnixStream>, LinesCodec>>,
    queue_full: Arc<AtomicBool>,
    spoke: bool,
    tree: Tree,
    updates: u32,
    window: Instant,
    hellos: VecDeque<Instant>,
    started: bool,
    done: bool,
}

impl Drop for SlotStream {
    fn drop(&mut self) {
        self.stop.cancel();
        self.connection.cancel();
        let launcher = Arc::clone(&self.launcher);
        let unit = self.unit.clone();
        let socket = self.socket.clone();
        let socket_id = self.socket_id;
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                launcher.stop(&unit).await;
                unlink_if_ours(&socket, socket_id);
            });
        }
    }
}

fn unlink_if_ours(socket: &std::path::Path, id: Option<(u64, u64)>) {
    let Some((dev, ino)) = id else {
        return;
    };
    if std::fs::metadata(socket).is_ok_and(|meta| meta.dev() == dev && meta.ino() == ino) {
        let _ = std::fs::remove_file(socket);
    }
}

pub async fn source(catalog: Arc<dyn Catalog>, args: SourceArgs) -> impl Stream<Item = Event> {
    stream::unfold(
        SlotStream {
            catalog,
            slot: args.slot,
            spawn: args.spawn,
            id: args.id,
            env: args.env,
            state_base: args.state_base,
            entry: args.entry,
            cwd: args.cwd,
            restart: args.restart,
            stderr: args.stderr,
            unit: args.unit,
            socket: args.socket,
            socket_id: None,
            launcher: args.launcher,
            stop: args.stop,
            connection: CancellationToken::new(),
            hello_state: String::new(),
            resolved: None,
            catalog_refused: false,
            listener: None,
            watch: None,
            reader: None,
            queue_full: Arc::new(AtomicBool::new(false)),
            spoke: false,
            tree: Tree::default(),
            updates: 0,
            window: Instant::now(),
            hellos: VecDeque::new(),
            started: false,
            done: false,
        },
        |mut source| async move { source.next().await.map(|event| (event, source)) },
    )
}

impl SlotStream {
    async fn throttle(&mut self) {
        if self.window.elapsed() >= Duration::from_secs(1) {
            self.window = Instant::now();
            self.updates = 0;
        }
        if self.updates >= MAX_UPDATES_PER_SECOND {
            tokio::select! {
                _ = tokio::time::sleep_until((self.window + Duration::from_secs(1)).into()) => {}
                _ = self.stop.cancelled() => {}
                _ = self.connection.cancelled() => {}
            }
            self.window = Instant::now();
            self.updates = 0;
        }
    }

    async fn next(&mut self) -> Option<Event> {
        loop {
            if self.done {
                return None;
            }
            if !self.started {
                return self.start().await;
            }
            if self.reader.is_none() {
                if let Some(event) = self.accept().await {
                    return Some(event);
                }
                continue;
            }
            self.throttle().await;
            if self.stop.is_cancelled() {
                self.done = true;
                return None;
            }
            if let Some(event) = self.read().await {
                return Some(event);
            }
        }
    }

    fn unit_event(&mut self, state: UnitState) -> Event {
        if matches!(state, UnitState::Gone) {
            self.watch = None;
        }
        Event::Unit {
            slot: self.slot,
            spawn: self.spawn,
            state,
        }
    }

    async fn accept(&mut self) -> Option<Event> {
        let Some(listener) = self.listener.as_ref() else {
            self.done = true;
            return None;
        };
        enum Next {
            Stop,
            Unit(UnitState),
            Accepted(std::io::Result<(UnixStream, tokio::net::unix::SocketAddr)>),
        }
        let stop = self.stop.clone();
        let mut watch = self.watch.take();
        let result = tokio::select! {
            biased;
            _ = stop.cancelled() => Next::Stop,
            state = async {
                match watch.as_mut() {
                    Some(watch) => watch.next().await.unwrap_or(UnitState::Gone),
                    None => std::future::pending().await,
                }
            } => Next::Unit(state),
            accepted = listener.accept() => Next::Accepted(accepted),
        };
        self.watch = watch;
        match result {
            Next::Stop => {
                self.done = true;
                None
            }
            Next::Unit(state) => Some(self.unit_event(state)),
            Next::Accepted(Ok((stream, _))) => {
                let link = self.connect(stream);
                Some(Event::Connected {
                    slot: self.slot,
                    spawn: self.spawn,
                    link,
                })
            }
            Next::Accepted(Err(error)) => {
                tracing::warn!(applet = %self.id, %error, "applet socket accept failed");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {}
                    _ = self.stop.cancelled() => {}
                }
                None
            }
        }
    }

    fn disconnect(&mut self) {
        self.reader = None;
        self.connection.cancel();
        self.connection = CancellationToken::new();
        self.spoke = false;
    }

    async fn misbehave(&mut self, reason: String) -> Option<Event> {
        tracing::warn!(applet = %self.id, %reason, "applet misbehaved, killing its unit");
        self.disconnect();
        self.launcher.kill(&self.unit).await;
        Some(Event::Misbehaved {
            slot: self.slot,
            spawn: self.spawn,
            reason,
        })
    }

    async fn read(&mut self) -> Option<Event> {
        enum Read {
            Stop,
            Connection,
            Unit(UnitState),
            Extra,
            Line(Option<Result<String, tokio_util::codec::LinesCodecError>>),
        }
        let reader = self.reader.as_mut()?;
        let stop = self.stop.clone();
        let connection = self.connection.clone();
        let mut watch = self.watch.take();
        let listener = self.listener.as_ref();
        let result = tokio::select! {
            biased;
            _ = stop.cancelled() => Read::Stop,
            _ = connection.cancelled() => Read::Connection,
            line = reader.next() => Read::Line(line),
            state = async {
                match watch.as_mut() {
                    Some(watch) => watch.next().await.unwrap_or(UnitState::Gone),
                    None => std::future::pending().await,
                }
            } => Read::Unit(state),
            accepted = async {
                match listener {
                    Some(listener) => listener.accept().await.map(|_| ()),
                    None => std::future::pending().await,
                }
            } => {
                let _ = accepted;
                Read::Extra
            }
        };
        self.watch = watch;
        if matches!(result, Read::Line(Some(Ok(_)))) {
            self.updates += 1;
        }
        match result {
            Read::Stop => {
                self.done = true;
                None
            }
            Read::Extra => None,
            Read::Unit(state) => Some(self.unit_event(state)),
            Read::Connection => {
                let reason = if self.queue_full.load(Ordering::Relaxed) {
                    "not reading its socket".to_owned()
                } else {
                    "connection stopped".to_owned()
                };
                self.misbehave(reason).await
            }
            Read::Line(None) => {
                self.disconnect();
                Some(Event::Disconnected {
                    slot: self.slot,
                    spawn: self.spawn,
                })
            }
            Read::Line(Some(Err(error))) => self.misbehave(format!("invalid line: {error}")).await,
            Read::Line(Some(Ok(line))) => match serde_json::from_str::<FromApplet>(&line) {
                Ok(FromApplet::Unknown) => {
                    tracing::debug!(applet = %self.id, "ignored an unknown message");
                    None
                }
                Ok(FromApplet::Hello { v: 1 }) => {
                    let now = Instant::now();
                    self.hellos
                        .retain(|at| now.duration_since(*at) < EXEC_HELLO_WINDOW);
                    self.hellos.push_back(now);
                    if self.hellos.len() > EXEC_MAX_HELLOS {
                        return self.misbehave("too many hellos".to_owned()).await;
                    }
                    self.spoke = true;
                    self.tree = Tree::default();
                    Some(Event::Hello {
                        slot: self.slot,
                        spawn: self.spawn,
                        state: self.hello_state.clone(),
                    })
                }
                Ok(FromApplet::Hello { v }) => {
                    self.misbehave(format!("unsupported protocol version {v}"))
                        .await
                }
                Ok(FromApplet::Commit { ops }) if self.spoke => match self.tree.apply(ops) {
                    Ok(tree) => {
                        self.tree = tree;
                        Some(Event::Tree {
                            slot: self.slot,
                            spawn: self.spawn,
                            tree: Arc::new(self.tree.clone()),
                        })
                    }
                    Err(error) => self.misbehave(format!("invalid tree: {error}")).await,
                },
                Ok(FromApplet::Commit { .. }) => {
                    self.misbehave("commit before hello".to_owned()).await
                }
                Ok(request) if self.spoke => Some(Event::Request {
                    slot: self.slot,
                    spawn: self.spawn,
                    request,
                }),
                Ok(_) => self.misbehave("request before hello".to_owned()).await,
                Err(error) => self.misbehave(format!("invalid message: {error}")).await,
            },
        }
    }

    async fn start(&mut self) -> Option<Event> {
        self.started = true;
        let Some(base) = self.state_base.clone() else {
            return self.failed("cannot determine applet state directory".to_owned());
        };
        let id = self.id.clone();
        let state =
            match tokio::task::spawn_blocking(move || super::exec_state_dir(&base, &id)).await {
                Ok(Ok(state)) => state,
                Ok(Err(reason)) => return self.failed(reason),
                Err(error) => {
                    return self.failed(format!("cannot create applet state directory: {error}"));
                }
            };
        let Some(hello_state) = state.to_str() else {
            return self.failed("cannot determine applet state directory".to_owned());
        };
        self.hello_state = hello_state.to_owned();
        let entry = match self.entry.clone() {
            Some(entry) => entry,
            None => {
                let catalog = Arc::clone(&self.catalog);
                let id = self.id.clone();
                match tokio::task::spawn_blocking(move || catalog.resolve(&id)).await {
                    Ok(Ok(entry)) => entry,
                    Ok(Err(reason)) => {
                        self.catalog_refused = true;
                        return self.failed(reason);
                    }
                    Err(error) => return self.failed(format!("cannot resolve applet: {error}")),
                }
            }
        };
        self.resolved = Some(entry.clone());
        if let Some(parent) = self.socket.parent()
            && let Err(error) = tokio::fs::create_dir_all(parent).await
        {
            return self.failed(format!("cannot create applet socket directory: {error}"));
        }
        let _ = tokio::fs::remove_file(&self.socket).await;
        let listener = match UnixListener::bind(&self.socket) {
            Ok(listener) => listener,
            Err(error) => return self.failed(format!("cannot bind applet socket: {error}")),
        };
        self.socket_id = std::fs::metadata(&self.socket)
            .ok()
            .map(|meta| (meta.dev(), meta.ino()));
        self.listener = Some(listener);
        if self.stop.is_cancelled() {
            self.done = true;
            return None;
        }
        let cwd = self.cwd.clone().or_else(|| entry.cwd.clone());
        let socket = self.socket.clone();
        let env = self.env.clone();
        let unit = self.unit.clone();
        let description = entry.name.clone();
        let argv = entry.argv.clone();
        let restart = self.restart;
        let spec = match tokio::task::spawn_blocking(move || {
            build_spec(
                unit,
                description,
                argv,
                cwd,
                panel_env(),
                &socket,
                &env,
                restart,
                None,
            )
        })
        .await
        {
            Ok(Ok(spec)) => spec,
            Ok(Err(reason)) => return self.failed(reason),
            Err(error) => return self.failed(format!("cannot build applet unit: {error}")),
        };
        let spec = super::launcher::UnitSpec {
            stderr: self.stderr.take(),
            ..spec
        };
        if let Err(reason) = self.launcher.start(spec).await {
            return self.failed(reason);
        }
        self.watch = Some(self.launcher.watch(&self.unit));
        tracing::info!(applet = %self.id, slot = self.slot, unit = %self.unit, "applet unit started");
        Some(Event::Started {
            slot: self.slot,
            spawn: self.spawn,
            entry,
        })
    }

    fn connect(&mut self, stream: UnixStream) -> Link {
        let (reader, writer) = tokio::io::split(stream);
        self.connection = CancellationToken::new();
        let stop = self.connection.clone();
        let slot_stop = self.stop.clone();
        let (tx, mut rx) = mpsc::channel::<Outgoing>(64);
        tokio::spawn(async move {
            let mut writer = FramedWrite::new(writer, LinesCodec::new());
            while let Some(message) = tokio::select! {
                biased;
                _ = stop.cancelled() => None,
                _ = slot_stop.cancelled() => None,
                message = rx.recv() => message,
            } {
                let Ok(line) = serde_json::to_string(&message) else {
                    stop.cancel();
                    break;
                };
                let sent = tokio::select! {
                    biased;
                    _ = stop.cancelled() => false,
                    _ = slot_stop.cancelled() => false,
                    result = writer.send(line) => result.is_ok(),
                };
                if !sent {
                    stop.cancel();
                    break;
                }
            }
        });
        self.reader = Some(FramedRead::new(
            reader,
            LinesCodec::new_with_max_length(MAX_LINE),
        ));
        self.spoke = false;
        Link {
            tx,
            stop: self.connection.clone(),
            queue_full: Arc::clone(&self.queue_full),
        }
    }

    fn failed(&mut self, reason: String) -> Option<Event> {
        self.done = true;
        Some(Event::Exited {
            slot: self.slot,
            spawn: self.spawn,
            reason,
            resolved: self.resolved.clone(),
            catalog_refused: self.catalog_refused,
        })
    }
}
