mod identify;
mod model;
mod pulse;
#[cfg(test)]
mod tests;

pub use model::*;

use tokio::sync::oneshot;

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, NoConfig, Service, ServiceError},
    subscription::Sub,
};

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Watch {
    Pulse(u64),
}

type Reply = oneshot::Sender<Result<(), AudioError>>;

pub enum Command {
    SetDeviceVolume {
        dir: Direction,
        id: DeviceId,
        percent: u32,
        reply: Reply,
    },
    SetDeviceMuted {
        dir: Direction,
        id: DeviceId,
        muted: bool,
        reply: Reply,
    },
    SetDefault {
        dir: Direction,
        id: DeviceId,
        reply: Reply,
    },
    SetAppVolume {
        dir: Direction,
        app: AppId,
        percent: u32,
        reply: Reply,
    },
    SetAppMuted {
        dir: Direction,
        app: AppId,
        muted: bool,
        reply: Reply,
    },
    MoveApp {
        dir: Direction,
        app: AppId,
        to: DeviceId,
        reply: Reply,
    },
}

pub struct Audio {
    state: Publisher<AudioState>,
    current: AudioState,
    client: Option<pulse::Client>,
    generation: u64,
}

#[derive(Clone)]
pub struct AudioHandle(crate::ServiceEndpoint<Audio>);

impl AudioHandle {
    pub fn snapshot(&self) -> AudioState {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<AudioState> {
        self.0.subscribe()
    }

    pub fn health(&self) -> tokio::sync::watch::Receiver<crate::ServiceState> {
        self.0.health()
    }

    pub async fn set_device_volume(
        &self,
        dir: Direction,
        id: DeviceId,
        percent: u32,
    ) -> Result<(), AudioError> {
        self.call(|reply| Command::SetDeviceVolume {
            dir,
            id,
            percent,
            reply,
        })
        .await
    }

    pub async fn set_device_muted(
        &self,
        dir: Direction,
        id: DeviceId,
        muted: bool,
    ) -> Result<(), AudioError> {
        self.call(|reply| Command::SetDeviceMuted {
            dir,
            id,
            muted,
            reply,
        })
        .await
    }

    pub async fn set_default(&self, dir: Direction, id: DeviceId) -> Result<(), AudioError> {
        self.call(|reply| Command::SetDefault { dir, id, reply })
            .await
    }

    pub async fn set_app_volume(
        &self,
        dir: Direction,
        app: AppId,
        percent: u32,
    ) -> Result<(), AudioError> {
        self.call(|reply| Command::SetAppVolume {
            dir,
            app,
            percent,
            reply,
        })
        .await
    }

    pub async fn set_app_muted(
        &self,
        dir: Direction,
        app: AppId,
        muted: bool,
    ) -> Result<(), AudioError> {
        self.call(|reply| Command::SetAppMuted {
            dir,
            app,
            muted,
            reply,
        })
        .await
    }

    pub async fn move_app(
        &self,
        dir: Direction,
        app: AppId,
        to: DeviceId,
    ) -> Result<(), AudioError> {
        self.call(|reply| Command::MoveApp {
            dir,
            app,
            to,
            reply,
        })
        .await
    }

    async fn call(&self, command: impl FnOnce(Reply) -> Command) -> Result<(), AudioError> {
        let (reply, result) = oneshot::channel();
        self.0.command(command(reply))?;
        result.await.map_err(|_| {
            CommandError::Unavailable("audio stopped before completing the command".to_owned())
        })?
    }
}

impl Service for Audio {
    const NAME: &'static str = "audio";
    type Config = NoConfig;
    type State = AudioState;
    type Handle = AudioHandle;
    type Command = Command;
    type Event = pulse::Event;
    type Dependencies = ();
    type SubKey = Watch;

    fn from_endpoint(endpoint: crate::ServiceEndpoint<Self>) -> Self::Handle {
        AudioHandle(endpoint)
    }

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        vec![Sub::stream(Watch::Pulse(self.generation), |_ctx| async {
            pulse::connect().await
        })]
    }

    async fn start(
        ctx: &Ctx<Self>,
        _config: Self::Config,
        _: Self::Dependencies,
    ) -> Result<Self, ServiceError> {
        Ok(Self {
            state: ctx.publisher(),
            current: AudioState::default(),
            client: None,
            generation: 0,
        })
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Command(command) => self.run(ctx, command).await,
            Input::Config(_) => {}
            Input::Event(pulse::Event::Ready(client)) => {
                self.client = Some(client);
                ctx.running();
            }
            Input::Event(pulse::Event::Snapshot(snapshot)) => {
                let pulse::Snapshot {
                    outputs,
                    inputs,
                    apps,
                } = *snapshot;
                self.current = AudioState {
                    outputs,
                    inputs,
                    apps,
                };
                self.state.set(self.current.clone());
            }
            Input::Event(pulse::Event::Gone(reason)) => {
                self.client = None;
                self.generation = self.generation.wrapping_add(1);
                ctx.degraded(reason);
            }
        }
    }
}

impl Audio {
    async fn run(&mut self, ctx: &Ctx<Self>, command: Command) {
        match command {
            Command::SetDeviceVolume {
                dir,
                id,
                percent,
                reply,
            } => {
                let Some(client) = self.client.clone() else {
                    let _ = reply.send(Err(AudioError::Unavailable));
                    return;
                };
                let Some(index) = self.device_index(dir, &id) else {
                    let _ = reply.send(Err(refused("device")));
                    return;
                };
                detached(ctx, reply, async move {
                    client
                        .send(|reply| pulse::Request::SetDeviceVolume {
                            dir,
                            index,
                            percent,
                            reply,
                        })
                        .await
                });
            }
            Command::SetDeviceMuted {
                dir,
                id,
                muted,
                reply,
            } => {
                let Some(client) = self.client.clone() else {
                    let _ = reply.send(Err(AudioError::Unavailable));
                    return;
                };
                let Some(index) = self.device_index(dir, &id) else {
                    let _ = reply.send(Err(refused("device")));
                    return;
                };
                detached(ctx, reply, async move {
                    client
                        .send(|reply| pulse::Request::SetDeviceMuted {
                            dir,
                            index,
                            muted,
                            reply,
                        })
                        .await
                });
            }
            Command::SetDefault { dir, id, reply } => {
                let Some(client) = self.client.clone() else {
                    let _ = reply.send(Err(AudioError::Unavailable));
                    return;
                };
                if self.current.device(dir, &id).is_none() {
                    let _ = reply.send(Err(refused("device")));
                    return;
                }
                let name = id.as_str().to_owned();
                detached(ctx, reply, async move {
                    client
                        .send(|reply| pulse::Request::SetDefault { dir, name, reply })
                        .await
                });
            }
            Command::SetAppVolume {
                dir,
                app,
                percent,
                reply,
            } => {
                let Some(client) = self.client.clone() else {
                    let _ = reply.send(Err(AudioError::Unavailable));
                    return;
                };
                let Some(role) = self.app_role(dir, &app) else {
                    let _ = reply.send(Err(refused("application")));
                    return;
                };
                let streams = role.scaled(percent);
                detached(ctx, reply, async move {
                    client
                        .send(|reply| pulse::Request::SetStreamVolume {
                            dir,
                            streams,
                            reply,
                        })
                        .await
                });
            }
            Command::SetAppMuted {
                dir,
                app,
                muted,
                reply,
            } => {
                let Some(client) = self.client.clone() else {
                    let _ = reply.send(Err(AudioError::Unavailable));
                    return;
                };
                let Some(role) = self.app_role(dir, &app) else {
                    let _ = reply.send(Err(refused("application")));
                    return;
                };
                let streams = stream_indices(&role);
                detached(ctx, reply, async move {
                    client
                        .send(|reply| pulse::Request::SetStreamMuted {
                            dir,
                            streams,
                            muted,
                            reply,
                        })
                        .await
                });
            }
            Command::MoveApp {
                dir,
                app,
                to,
                reply,
            } => {
                let Some(client) = self.client.clone() else {
                    let _ = reply.send(Err(AudioError::Unavailable));
                    return;
                };
                let Some(role) = self.app_role(dir, &app) else {
                    let _ = reply.send(Err(refused("application")));
                    return;
                };
                let Some(target) = self.device_index(dir, &to) else {
                    let _ = reply.send(Err(refused("device")));
                    return;
                };
                let streams = stream_indices(&role);
                detached(ctx, reply, async move {
                    client
                        .send(|reply| pulse::Request::MoveStreams {
                            dir,
                            streams,
                            target,
                            reply,
                        })
                        .await
                });
            }
        }
    }

    fn device_index(&self, dir: Direction, id: &DeviceId) -> Option<u32> {
        self.current.device(dir, id).map(|device| device.index)
    }

    fn app_role(&self, dir: Direction, id: &AppId) -> Option<Role> {
        self.current.app(id)?.role(dir).cloned()
    }
}

fn detached(
    ctx: &Ctx<Audio>,
    reply: Reply,
    work: impl Future<Output = Result<(), AudioError>> + Send + 'static,
) {
    ctx.spawn_detached(move |_ctx| async move {
        let _ = reply.send(work.await);
    });
}

fn refused(what: &str) -> AudioError {
    AudioError::Refused(format!("there is no such {what}"))
}

fn stream_indices(role: &Role) -> Vec<u32> {
    role.streams.iter().map(|stream| stream.index).collect()
}
