use std::os::unix::process::ExitStatusExt;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, Stream, StreamExt, stream};
use tokio::process::{Child, ChildStdout, Command};
use tokio::sync::mpsc;
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};
use tokio_util::sync::CancellationToken;

use crate::Ctx;

use super::{
    Catalog, Event, Exec, FromApplet, Link, MAX_LINE, MAX_UPDATES_PER_SECOND, Outgoing, Tree, scope,
};

struct ChildStream {
    ctx: Ctx<Exec>,
    catalog: Arc<dyn Catalog>,
    slot: u64,
    spawn: u64,
    id: String,
    resolved: Option<super::Entry>,
    child: Option<Child>,
    group: Option<rustix::process::Pid>,
    scope_warned: Arc<AtomicBool>,
    queue_full: Arc<AtomicBool>,
    stdout: Option<FramedRead<ChildStdout, LinesCodec>>,
    stop: CancellationToken,
    began: Instant,
    spoke: bool,
    tree: Tree,
    updates: u32,
    window: Instant,
    done: bool,
}

pub(super) struct SourceArgs {
    pub slot: u64,
    pub spawn: u64,
    pub id: String,
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
            resolved: None,
            child: None,
            group: None,
            scope_warned: args.scope_warned,
            queue_full: Arc::new(AtomicBool::new(false)),
            stdout: None,
            stop: args.stop,
            began: Instant::now(),
            spoke: false,
            tree: Tree::default(),
            updates: 0,
            window: Instant::now(),
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
    }

    async fn next(&mut self) -> Option<Event> {
        loop {
            if self.done {
                return None;
            }
            if self.child.is_none() {
                return self.start().await;
            }
            self.throttle().await;
            if let Some(event) = self.read().await {
                return Some(event);
            }
        }
    }

    async fn read(&mut self) -> Option<Event> {
        let (Some(child), Some(stdout)) = (self.child.as_mut(), self.stdout.as_mut()) else {
            self.done = true;
            return None;
        };
        let result = tokio::select! {
            biased;
            line = stdout.next() => Read::Line(line),
            _ = self.stop.cancelled() => Read::Stop,
            status = child.wait() => Read::Exit(status.map(exit_reason).unwrap_or_else(|error| error.to_string())),
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
                    self.spoke = true;
                    self.tree = Tree::default();
                    Some(Event::Hello {
                        slot: self.slot,
                        spawn: self.spawn,
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
        let catalog = Arc::clone(&self.catalog);
        let id = self.id.clone();
        let entry = match tokio::task::spawn_blocking(move || catalog.resolve(&id)).await {
            Ok(Ok(entry)) => entry,
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
            .stderr(Stdio::null())
            .process_group(0)
            .kill_on_drop(true);
        if let Some(cwd) = &entry.cwd {
            command.current_dir(cwd);
        }
        let parent = rustix::process::getpid();
        unsafe {
            command.pre_exec(move || {
                rustix::process::set_parent_process_death_signal(Some(
                    rustix::process::Signal::KILL,
                ))?;
                if rustix::process::getppid() != Some(parent) {
                    return Err(rustix::io::Errno::SRCH.into());
                }
                Ok(())
            });
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => return self.failed(format!("cannot start applet: {error}")),
        };
        self.began = Instant::now();
        if let Some(pid) = child.id() {
            tracing::info!(applet = %self.id, slot = self.slot, pid, "applet spawned");
            self.group = rustix::process::Pid::from_raw(pid as i32);
            scope::adopt(&self.ctx, pid, &self.id, &self.scope_warned).await;
        }
        let Some(stdin) = child.stdin.take() else {
            return self.failed("stdin unavailable".to_owned());
        };
        let Some(stdout) = child.stdout.take() else {
            return self.failed("stdout unavailable".to_owned());
        };
        let (tx, mut rx) = mpsc::channel::<Outgoing>(64);
        let stop = self.stop.clone();
        tokio::spawn(async move {
            let mut writer = FramedWrite::new(stdin, LinesCodec::new());
            while let Some(message) = rx.recv().await {
                let Ok(line) = serde_json::to_string(&message) else {
                    stop.cancel();
                    break;
                };
                if writer.send(line).await.is_err() {
                    stop.cancel();
                    break;
                }
            }
        });
        self.stdout = Some(FramedRead::new(
            stdout,
            LinesCodec::new_with_max_length(MAX_LINE),
        ));
        self.child = Some(child);
        Some(Event::Spawned {
            slot: self.slot,
            spawn: self.spawn,
            link: Link {
                tx,
                stop: self.stop.clone(),
                queue_full: Arc::clone(&self.queue_full),
            },
            entry,
        })
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
