mod graph;
mod screencast;
mod source;

use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::time::SystemTime;

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{Input, NoConfig, Service},
    services::audio::{AudioHandle, AudioState},
    services::compositor::{CompositorHandle, CompositorPrivacy, CompositorState},
    subscription::Sub,
};

const NAME_CAP: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Resource {
    Camera,
    Microphone,
    Screen,
    Location,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Usage {
    pub kind: Resource,
    pub app: Option<String>,
    pub icon: Option<String>,
    pub detail: Option<String>,
    pub since: SystemTime,
    pub stream_id: Option<u64>,
    pub session: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PrivacyState {
    pub usages: Vec<Usage>,
}

#[derive(Debug, PartialEq)]
pub struct Fact {
    app: Option<String>,
    icon: Option<String>,
    detail: Option<String>,
    stream_id: Option<u64>,
    session: Option<u64>,
}

fn refresh(previous: &[Usage], kind: Resource, facts: Vec<Fact>) -> Vec<Usage> {
    facts
        .into_iter()
        .map(|fact| {
            let since = previous
                .iter()
                .find(|usage| {
                    usage.stream_id == fact.stream_id
                        && usage.app == fact.app
                        && usage.detail == fact.detail
                })
                .map_or_else(SystemTime::now, |usage| usage.since);
            Usage {
                kind,
                app: fact.app,
                icon: fact.icon,
                detail: fact.detail,
                since,
                stream_id: fact.stream_id,
                session: fact.session,
            }
        })
        .collect()
}

#[derive(PartialEq, Eq, Hash)]
pub enum Watch {
    Camera,
    Microphone,
    Screen,
    ScreenAttribution,
    Location,
}

pub enum Event {
    Camera(Vec<Fact>),
    Audio(AudioState),
    Compositor(Option<CompositorPrivacy>),
    Graph(HashMap<u32, String>),
    Location(bool),
    Unavailable(Resource, &'static str),
}

pub struct Dependencies {
    pub audio: AudioHandle,
    pub compositor: CompositorHandle,
}

pub struct Privacy {
    state: Publisher<PrivacyState>,
    audio: AudioHandle,
    compositor: CompositorHandle,
    camera: Vec<Usage>,
    microphone: Vec<Usage>,
    screen: Vec<Usage>,
    location: Vec<Usage>,
    compositor_privacy: Option<CompositorPrivacy>,
    attribution: HashMap<u32, String>,
}

pub type PrivacyHandle = crate::ServiceEndpoint<Privacy>;

impl Service for Privacy {
    const NAME: &'static str = "privacy";
    type Config = NoConfig;
    type State = PrivacyState;
    type Command = Infallible;
    type Event = Event;
    type Dependencies = Dependencies;
    type SubKey = Watch;

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        vec![
            Sub::stream(Watch::Camera, |_ctx| async {
                source::camera(PathBuf::from("/dev"), PathBuf::from("/proc"))
            }),
            Sub::watch(
                Watch::Microphone,
                self.audio.subscribe(),
                Event::Audio,
                Event::Unavailable(
                    Resource::Microphone,
                    "microphone: the audio service is unavailable",
                ),
            ),
            Sub::watch(
                Watch::Screen,
                self.compositor.subscribe(),
                |state: CompositorState| Event::Compositor(state.privacy),
                Event::Unavailable(
                    Resource::Screen,
                    "screen: the compositor service is unavailable",
                ),
            ),
            Sub::stream(Watch::ScreenAttribution, |_ctx| screencast::attribution()),
            Sub::stream(Watch::Location, source::location),
        ]
    }

    async fn start(ctx: &Ctx<Self>, _config: Self::Config, deps: Self::Dependencies) -> Self {
        Self {
            state: ctx.publisher(),
            audio: deps.audio,
            compositor: deps.compositor,
            camera: Vec::new(),
            microphone: Vec::new(),
            screen: Vec::new(),
            location: Vec::new(),
            compositor_privacy: None,
            attribution: HashMap::new(),
        }
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Command(command) => match command {},
            Input::Config(NoConfig) => {}
            Input::Event(Event::Camera(facts)) => {
                ctx.running();
                self.camera = refresh(&self.camera, Resource::Camera, facts);
                self.publish();
            }
            Input::Event(Event::Audio(state)) => {
                ctx.running();
                let facts = source::microphone(&state);
                self.microphone = refresh(&self.microphone, Resource::Microphone, facts);
                self.publish();
            }
            Input::Event(Event::Compositor(privacy)) => {
                ctx.running();
                self.compositor_privacy = privacy;
                self.recompute_screen();
                self.publish();
            }
            Input::Event(Event::Graph(attribution)) => {
                self.attribution = attribution;
                self.recompute_screen();
                self.publish();
            }
            Input::Event(Event::Location(in_use)) => {
                ctx.running();
                let facts = if in_use {
                    vec![Fact {
                        app: None,
                        icon: None,
                        detail: None,
                        stream_id: None,
                        session: None,
                    }]
                } else {
                    Vec::new()
                };
                self.location = refresh(&self.location, Resource::Location, facts);
                self.publish();
            }
            Input::Event(Event::Unavailable(resource, reason)) => {
                ctx.degraded(reason);
                match resource {
                    Resource::Camera => self.camera.clear(),
                    Resource::Microphone => self.microphone.clear(),
                    Resource::Screen => self.screen.clear(),
                    Resource::Location => self.location.clear(),
                }
                self.publish();
            }
        }
    }
}

impl Privacy {
    fn recompute_screen(&mut self) {
        let facts = self
            .compositor_privacy
            .as_ref()
            .map(|privacy| source::screen(privacy, &self.attribution))
            .unwrap_or_default();
        self.screen = refresh(&self.screen, Resource::Screen, facts);
    }

    fn publish(&mut self) {
        let mut usages = Vec::with_capacity(
            self.camera.len() + self.microphone.len() + self.screen.len() + self.location.len(),
        );
        usages.extend(self.camera.iter().cloned());
        usages.extend(self.microphone.iter().cloned());
        usages.extend(self.screen.iter().cloned());
        usages.extend(self.location.iter().cloned());
        self.state.set(PrivacyState { usages });
    }
}

#[cfg(test)]
mod tests;
