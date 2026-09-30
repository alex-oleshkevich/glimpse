use std::collections::VecDeque;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, Stream, StreamExt, stream};
use rustix::fs::OFlags;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::unix::pipe;
use tokio::process::{Child, ChildStderr, Command};
use tokio::sync::{Mutex, mpsc};
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec, LinesCodecError};
use tokio_util::sync::CancellationToken;

use crate::Ctx;

use super::{
    Catalog, Event, Exec, FromApplet, Link, MAX_LINE, MAX_UPDATES_PER_SECOND, Origin, Outgoing,
    Pipes, Tree, sandbox, scope,
};

struct ChildStream {
    ctx: Ctx<Exec>,
    catalog: Arc<dyn Catalog>,
    slot: u64,
    spawn: u64,
    id: String,
    sandbox: glimpse_config::AppletSandbox,
    state: Option<PathBuf>,
    state_base: Option<PathBuf>,
    origin: Origin,
    pipes: Option<Arc<Mutex<Option<Pipes>>>>,
    hello_state: String,
    resolved: Option<super::Entry>,
    child: Option<Child>,
    group: Option<rustix::process::Pid>,
    scope: Option<String>,
    scope_warned: Arc<AtomicBool>,
    queue_full: Arc<AtomicBool>,
    stdout: Option<FramedRead<Box<dyn AsyncRead + Send + Unpin>, LinesCodec>>,
    stop: CancellationToken,
    began: Instant,
    spoke: bool,
    tree: Tree,
    updates: u32,
    window: Instant,
    hellos: VecDeque<Instant>,
    done: bool,
}

pub const EXEC_MAX_HELLOS: usize = 5;
pub const EXEC_HELLO_WINDOW: Duration = Duration::from_secs(10);
const STDERR_LINE: usize = 4096;
const STDERR_LINES_PER_SECOND: u32 = 50;

pub(super) struct SourceArgs {
    pub slot: u64,
    pub spawn: u64,
    pub id: String,
    pub sandbox: glimpse_config::AppletSandbox,
    pub state: Option<PathBuf>,
    pub state_base: Option<PathBuf>,
    pub origin: Origin,
    pub pipes: Option<Arc<Mutex<Option<Pipes>>>>,
    pub entry: Option<super::Entry>,
    pub stop: CancellationToken,
    pub scope_warned: Arc<AtomicBool>,
}

impl Drop for ChildStream {
    fn drop(&mut self) {
        self.stop.cancel();
        if let Some(mut child) = self.child.take() {
            self.kill_group();
            let _ = child.start_kill();
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    let _ = child.wait().await;
                });
            }
        }
    }
}

pub async fn source(
    ctx: Ctx<Exec>,
    catalog: Arc<dyn Catalog>,
    args: SourceArgs,
) -> impl Stream<Item = Event> {
    stream::unfold(
        ChildStream {
            ctx,
            catalog,
            slot: args.slot,
            spawn: args.spawn,
            id: args.id,
            sandbox: args.sandbox,
            state: args.state,
            state_base: args.state_base,
            origin: args.origin,
            pipes: args.pipes,
            hello_state: String::new(),
            resolved: args.entry,
            child: None,
            group: None,
            scope: None,
            scope_warned: args.scope_warned,
            queue_full: Arc::new(AtomicBool::new(false)),
            stdout: None,
            stop: args.stop,
            began: Instant::now(),
            spoke: false,
            tree: Tree::default(),
            updates: 0,
            window: Instant::now(),
            hellos: VecDeque::new(),
            done: false,
        },
        |mut source| async move { source.next().await.map(|event| (event, source)) },
    )
}

impl ChildStream {
    async fn throttle(&mut self) {
        if self.window.elapsed() >= Duration::from_secs(1) {
            self.window = Instant::now();
            self.updates = 0;
        }
        if self.updates >= MAX_UPDATES_PER_SECOND {
            tokio::select! {
                _ = tokio::time::sleep_until((self.window + Duration::from_secs(1)).into()) => {}
                _ = self.stop.cancelled() => {}
            }
            self.window = Instant::now();
            self.updates = 0;
        }
    }

    fn kill_group(&self) {
        if let Some(group) = self.group {
            let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
        }
        if let Some(unit) = self.scope.clone()
            && let Ok(bus) = self.ctx.session_bus()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            runtime.spawn(scope::kill(bus.clone(), unit));
        }
    }

    async fn next(&mut self) -> Option<Event> {
        loop {
            if self.done {
                return None;
            }
            if self.stdout.is_none() {
                return match self.origin {
                    Origin::Spawned => self.start().await,
                    Origin::Attached => self.start_attached().await,
                };
            }
            self.throttle().await;
            if let Some(event) = self.read().await {
                return Some(event);
            }
        }
    }

    async fn read(&mut self) -> Option<Event> {
        let Some(stdout) = self.stdout.as_mut() else {
            self.done = true;
            return None;
        };
        let result = if let Some(child) = self.child.as_mut() {
            tokio::select! {
                biased;
                line = stdout.next() => Read::Line(line),
                _ = self.stop.cancelled() => Read::Stop,
                status = child.wait() => Read::Exit(status.map(exit_reason).unwrap_or_else(|error| error.to_string())),
            }
        } else {
            tokio::select! {
                biased;
                line = stdout.next() => Read::Line(line),
                _ = self.stop.cancelled() => Read::Stop,
            }
        };
        if matches!(result, Read::Line(Some(Ok(_)))) {
            self.updates += 1;
        }
        match result {
            Read::Stop => {
                self.finish(
                    if self.queue_full.load(Ordering::Relaxed) {
                        "not reading stdin"
                    } else {
                        "stopped"
                    }
                    .to_owned(),
                    true,
                )
                .await
            }
            Read::Exit(reason) => self.finish(reason, false).await,
            Read::Line(None) => {
                if self.origin == Origin::Attached {
                    self.done = true;
                    self.stop.cancel();
                    return Some(Event::Ended {
                        slot: self.slot,
                        spawn: self.spawn,
                    });
                }
                let reason = match self.child.as_mut() {
                    Some(child) => {
                        match tokio::time::timeout(Duration::from_secs(1), child.wait()).await {
                            Ok(Ok(status)) => exit_reason(status),
                            Ok(Err(error)) => error.to_string(),
                            Err(_) => "stdout closed".to_owned(),
                        }
                    }
                    None => "stdout closed".to_owned(),
                };
                self.finish(reason, true).await
            }
            Read::Line(Some(Err(error))) => {
                self.finish(format!("invalid line: {error}"), true).await
            }
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
                        return self.finish("too many hellos".to_owned(), true).await;
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
                    self.finish(format!("unsupported protocol version {v}"), true)
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
                    Err(error) => self.finish(format!("invalid tree: {error}"), true).await,
                },
                Ok(FromApplet::Commit { .. }) => {
                    self.finish("commit before hello".to_owned(), true).await
                }
                Ok(request) if self.spoke => Some(Event::Request {
                    slot: self.slot,
                    spawn: self.spawn,
                    request,
                }),
                Ok(_) => self.finish("request before hello".to_owned(), true).await,
                Err(error) => self.finish(format!("invalid message: {error}"), true).await,
            },
        }
    }

    async fn start(&mut self) -> Option<Event> {
        let Some(state) = self.state.clone() else {
            return self.failed("cannot determine applet state directory".to_owned());
        };
        let Some(hello_state) = state.to_str() else {
            return self.failed("cannot determine applet state directory".to_owned());
        };
        self.hello_state = hello_state.to_owned();
        let catalog = Arc::clone(&self.catalog);
        let id = self.id.clone();
        let sandbox = self.sandbox.clone();
        let (entry, ruleset) = match tokio::task::spawn_blocking(move || {
            let entry = catalog.resolve(&id)?;
            let ruleset = sandbox::exec_sandbox(&sandbox, &id, &state)?;
            Ok::<_, String>((entry, ruleset))
        })
        .await
        {
            Ok(Ok(result)) => result,
            Ok(Err(reason)) => return self.failed(reason),
            Err(error) => return self.failed(format!("cannot resolve applet: {error}")),
        };
        self.resolved = Some(entry.clone());
        if self.stop.is_cancelled() {
            return self.failed("stopped".to_owned());
        }
        let Some(program) = entry.argv.first() else {
            return self.failed("empty argv".to_owned());
        };
        let mut command = Command::new(program);
        command
            .args(entry.argv.iter().skip(1))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .kill_on_drop(true);
        if let Some(cwd) = &entry.cwd {
            command.current_dir(cwd);
        }
        let parent = rustix::process::getpid();
        let ruleset_fd = ruleset.as_ref().map(AsRawFd::as_raw_fd);
        unsafe {
            command.pre_exec(move || sandbox::exec_confine(ruleset_fd, parent));
        }
        let spawn_result = command.spawn();
        let confined = ruleset.is_some();
        drop(ruleset);
        let mut child = match spawn_result {
            Ok(child) => child,
            Err(error) if confined && error.raw_os_error() == Some(libc::EACCES) => {
                return self.failed(format!(
                    "cannot start applet: {error} (not granted by sandbox)"
                ));
            }
            Err(error) => return self.failed(format!("cannot start applet: {error}")),
        };
        self.began = Instant::now();
        if let Some(pid) = child.id() {
            tracing::info!(applet = %self.id, slot = self.slot, pid, "applet spawned");
            self.group = rustix::process::Pid::from_raw(pid as i32);
            if scope::adopt(&self.ctx, pid, &self.id, &self.scope_warned).await {
                self.scope = Some(scope::unit_name(&self.id, pid));
            }
        }
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(drain(stderr, self.id.clone()));
        }
        let Some(stdin) = child.stdin.take() else {
            return self.failed("stdin unavailable".to_owned());
        };
        let Some(stdout) = child.stdout.take() else {
            return self.failed("stdout unavailable".to_owned());
        };
        let link = self.connect(stdout, stdin);
        self.child = Some(child);
        Some(Event::Spawned {
            slot: self.slot,
            spawn: self.spawn,
            link,
            entry,
        })
    }

    async fn start_attached(&mut self) -> Option<Event> {
        let Some(base) = self.state_base.clone() else {
            return self.failed("cannot determine applet state directory".to_owned());
        };
        let id = self.id.clone();
        let state =
            match tokio::task::spawn_blocking(move || sandbox::exec_state_dir(&base, &id)).await {
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
        let pipes = match &self.pipes {
            Some(pipes) => pipes.lock().await.take(),
            None => None,
        };
        let Some(pipes) = pipes else {
            return self.failed("pipes unavailable".to_owned());
        };
        self.pipes = None;
        if !opened_as(&pipes.from_applet, OFlags::RDONLY) {
            return self.failed("applet pipe must be opened read-only".to_owned());
        }
        if !opened_as(&pipes.to_applet, OFlags::WRONLY) {
            return self.failed("host pipe must be opened write-only".to_owned());
        }
        let from_applet = match pipe::Receiver::from_owned_fd(pipes.from_applet) {
            Ok(receiver) => receiver,
            Err(error) => return self.failed(format!("cannot read applet pipe: {error}")),
        };
        let to_applet = match pipe::Sender::from_owned_fd(pipes.to_applet) {
            Ok(sender) => sender,
            Err(error) => return self.failed(format!("cannot write applet pipe: {error}")),
        };
        let Some(entry) = self.resolved.clone() else {
            return self.failed("entry unavailable".to_owned());
        };
        let link = self.connect(from_applet, to_applet);
        Some(Event::Spawned {
            slot: self.slot,
            spawn: self.spawn,
            link,
            entry,
        })
    }

    fn connect(
        &mut self,
        reader: impl AsyncRead + Send + Unpin + 'static,
        writer: impl AsyncWrite + Send + Unpin + 'static,
    ) -> Link {
        let (tx, mut rx) = mpsc::channel::<Outgoing>(64);
        let stop = self.stop.clone();
        tokio::spawn(async move {
            let mut writer = FramedWrite::new(writer, LinesCodec::new());
            while let Some(message) = tokio::select! {
                biased;
                _ = stop.cancelled() => None,
                message = rx.recv() => message,
            } {
                let Ok(line) = serde_json::to_string(&message) else {
                    stop.cancel();
                    break;
                };
                let sent = tokio::select! {
                    biased;
                    _ = stop.cancelled() => false,
                    result = writer.send(line) => result.is_ok(),
                };
                if !sent {
                    stop.cancel();
                    break;
                }
            }
        });
        self.stdout = Some(FramedRead::new(
            Box::new(reader),
            LinesCodec::new_with_max_length(MAX_LINE),
        ));
        Link {
            tx,
            stop: self.stop.clone(),
            queue_full: Arc::clone(&self.queue_full),
        }
    }

    fn failed(&mut self, reason: String) -> Option<Event> {
        self.done = true;
        Some(Event::Exited {
            slot: self.slot,
            spawn: self.spawn,
            spoke: false,
            ran: Duration::ZERO,
            reason,
            resolved: self.resolved.clone(),
        })
    }

    async fn finish(&mut self, reason: String, kill: bool) -> Option<Event> {
        self.done = true;
        self.stop.cancel();
        if let Some(mut child) = self.child.take() {
            self.kill_group();
            if kill {
                let _ = child.start_kill();
            }
            let _ = child.wait().await;
        }
        Some(Event::Exited {
            slot: self.slot,
            spawn: self.spawn,
            spoke: self.spoke,
            ran: self.began.elapsed(),
            reason,
            resolved: self.resolved.clone(),
        })
    }
}

async fn drain(stderr: ChildStderr, id: String) {
    let mut lines = FramedRead::new(stderr, LinesCodec::new_with_max_length(STDERR_LINE));
    let mut window = Instant::now();
    let mut kept = 0;
    let mut dropped = 0;
    let mut errored = false;
    loop {
        let line = match lines.next().await {
            Some(Ok(line)) => line,
            Some(Err(LinesCodecError::MaxLineLengthExceeded)) => {
                errored = true;
                continue;
            }
            Some(Err(LinesCodecError::Io(error)))
                if error.kind() == std::io::ErrorKind::InvalidData =>
            {
                errored = true;
                continue;
            }
            None if std::mem::take(&mut errored) => continue,
            Some(Err(_)) | None => break,
        };
        errored = false;
        if window.elapsed() >= Duration::from_secs(1) {
            if dropped > 0 {
                tracing::warn!(target: "glimpse_applet", applet = %id, dropped, "applet stderr rate exceeded");
            }
            window = Instant::now();
            kept = 0;
            dropped = 0;
        }
        if kept < STDERR_LINES_PER_SECOND {
            kept += 1;
            tracing::info!(target: "glimpse_applet", applet = %id, "{}", glimpse_utils::clean(&line, 1024));
        } else {
            dropped += 1;
        }
    }
    if dropped > 0 {
        tracing::warn!(target: "glimpse_applet", applet = %id, dropped, "applet stderr rate exceeded");
    }
}

fn opened_as(fd: &OwnedFd, mode: OFlags) -> bool {
    rustix::fs::fcntl_getfl(fd).is_ok_and(|flags| flags & OFlags::RWMODE == mode)
}

fn exit_reason(status: ExitStatus) -> String {
    match (status.code(), status.signal()) {
        (Some(code), _) => format!("exited with status {code}"),
        (None, Some(signal)) => format!("killed by signal {signal}"),
        (None, None) => status.to_string(),
    }
}

enum Read {
    Stop,
    Exit(String),
    Line(Option<Result<String, tokio_util::codec::LinesCodecError>>),
}
