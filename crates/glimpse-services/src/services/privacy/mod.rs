mod graph;
mod screencast;
mod source;

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use tokio::sync::oneshot;

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{CommandError, Input, NoConfig, Service, ServiceError},
    services::audio::{AudioError, AudioHandle, AudioState, Direction},
    services::compositor::{CompositorHandle, CompositorPrivacy, CompositorState},
    subscription::Sub,
};

const NAME_CAP: usize = 128;
const CAMERA_POLL: Duration = Duration::from_secs(2);

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

type Reply = oneshot::Sender<Result<(), PrivacyError>>;

#[derive(Debug, thiserror::Error)]
pub enum PrivacyError {
    #[error(transparent)]
    Audio(#[from] AudioError),
    #[error(transparent)]
    Service(#[from] CommandError),
}

pub enum Command {
    MuteMicrophone { reply: Reply },
    StopScreencast { session_id: u64, reply: Reply },
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

#[derive(Clone)]
pub struct PrivacyHandle(crate::ServiceEndpoint<Privacy>);

impl PrivacyHandle {
    pub fn snapshot(&self) -> PrivacyState {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<PrivacyState> {
        self.0.subscribe()
    }

    pub fn health(&self) -> tokio::sync::watch::Receiver<crate::ServiceState> {
        self.0.health()
    }

    pub async fn mute_microphone(&self) -> Result<(), PrivacyError> {
        self.call(|reply| Command::MuteMicrophone { reply }).await
    }

    pub async fn stop_screencast(&self, session_id: u64) -> Result<(), PrivacyError> {
        self.call(|reply| Command::StopScreencast { session_id, reply })
            .await
    }

    async fn call(&self, command: impl FnOnce(Reply) -> Command) -> Result<(), PrivacyError> {
        let (reply, result) = oneshot::channel();
        self.0.command(command(reply))?;
        result.await.map_err(|_| {
            CommandError::Unavailable("privacy stopped before completing the command".to_owned())
        })?
    }
}

impl Service for Privacy {
    const NAME: &'static str = "privacy";
    type Config = NoConfig;
    type State = PrivacyState;
    type Handle = PrivacyHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = Dependencies;
    type SubKey = Watch;

    fn from_endpoint(endpoint: crate::ServiceEndpoint<Self>) -> Self::Handle {
        PrivacyHandle(endpoint)
    }

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        vec![
            Sub::interval(Watch::Camera, CAMERA_POLL, |_ctx| source::scan_camera()),
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

    async fn start(
        ctx: &Ctx<Self>,
        _config: Self::Config,
        deps: Self::Dependencies,
    ) -> Result<Self, ServiceError> {
        Ok(Self {
            state: ctx.publisher(),
            audio: deps.audio,
            compositor: deps.compositor,
            camera: Vec::new(),
            microphone: Vec::new(),
            screen: Vec::new(),
            location: Vec::new(),
            compositor_privacy: None,
            attribution: HashMap::new(),
        })
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Command(command) => self.dispatch(ctx, command),
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

    fn dispatch(&self, ctx: &Ctx<Self>, command: Command) {
        match command {
            Command::MuteMicrophone { reply } => self.mute_microphone(ctx, reply),
            Command::StopScreencast { session_id, reply } => {
                self.stop_screencast(ctx, session_id, reply)
            }
        }
    }

    fn mute_microphone(&self, ctx: &Ctx<Self>, reply: Reply) {
        let Some(id) = self
            .audio
            .snapshot()
            .default_input()
            .map(|device| device.id.clone())
        else {
            let _ = reply.send(Err(CommandError::Unavailable(
                "no default microphone".to_owned(),
            )
            .into()));
            return;
        };
        let audio = self.audio.clone();
        ctx.spawn_detached(move |_ctx| async move {
            let outcome = audio.set_device_muted(Direction::Input, id, true).await;
            let _ = reply.send(outcome.map_err(PrivacyError::from));
        });
    }

    fn stop_screencast(&self, ctx: &Ctx<Self>, session_id: u64, reply: Reply) {
        let compositor = self.compositor.clone();
        ctx.spawn_detached(move |_ctx| async move {
            let outcome = compositor.stop_screencast(session_id).await;
            let _ = reply.send(outcome.map_err(PrivacyError::from));
        });
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::sync::{mpsc, watch};
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::service::ServiceRuntime;
    use crate::services::audio::{
        App, AppId, Audio, Device as AudioDevice, DeviceId as AudioDeviceId, Role, StreamRef,
    };
    use crate::services::compositor::{CastInfo, CastKindInfo, CastTargetInfo, Compositor};
    use glimpse_dbus::Buses;

    fn usage(
        kind: Resource,
        app: Option<&str>,
        detail: Option<&str>,
        stream_id: Option<u64>,
        since: SystemTime,
    ) -> Usage {
        Usage {
            kind,
            app: app.map(str::to_owned),
            icon: None,
            detail: detail.map(str::to_owned),
            since,
            stream_id,
            session: None,
        }
    }

    fn fact(app: Option<&str>, detail: Option<&str>, stream_id: Option<u64>) -> Fact {
        Fact {
            app: app.map(str::to_owned),
            icon: None,
            detail: detail.map(str::to_owned),
            stream_id,
            session: None,
        }
    }

    #[test]
    fn a_new_fact_gets_a_fresh_since() {
        let before = SystemTime::now();
        let usages = refresh(
            &[],
            Resource::Camera,
            vec![fact(Some("ffmpeg"), None, None)],
        );

        assert_eq!(usages.len(), 1);
        assert!(usages[0].since >= before);
    }

    #[test]
    fn a_fact_matching_a_previous_one_keeps_its_since() {
        let original = SystemTime::now() - Duration::from_secs(120);
        let previous = vec![usage(
            Resource::Camera,
            Some("ffmpeg"),
            None,
            None,
            original,
        )];

        let usages = refresh(
            &previous,
            Resource::Camera,
            vec![fact(Some("ffmpeg"), None, None)],
        );

        assert_eq!(
            usages[0].since, original,
            "since must survive a republish for the same app, or every poll would read as a new \
             usage"
        );
    }

    #[test]
    fn two_window_casts_with_no_detail_are_tracked_independently_by_stream_id() {
        let older = SystemTime::now() - Duration::from_secs(60);
        let previous = vec![usage(Resource::Screen, None, None, Some(3), older)];

        let usages = refresh(
            &previous,
            Resource::Screen,
            vec![fact(None, None, Some(3)), fact(None, None, Some(7))],
        );

        assert_eq!(usages[0].since, older);
        assert!(
            usages[1].since > older,
            "two window casts share app: None and detail: None alike; without keying on \
             stream_id a second, distinct cast would wrongly inherit the first one's since"
        );
    }

    #[test]
    fn the_initial_state_has_no_usages() {
        assert_eq!(Privacy::initial_state(&NoConfig), PrivacyState::default());
        assert!(PrivacyState::default().usages.is_empty());
    }

    fn buses() -> Buses {
        Buses::unavailable("no bus in tests")
    }

    fn fake_audio(cancel: &CancellationToken) -> AudioHandle {
        let (runtime, handle) = ServiceRuntime::<Audio>::new(NoConfig, buses(), cancel.clone());
        drop(runtime);
        handle
    }

    fn fake_compositor(cancel: &CancellationToken) -> CompositorHandle {
        let (runtime, handle) =
            ServiceRuntime::<Compositor>::new(NoConfig, buses(), cancel.clone());
        drop(runtime);
        handle
    }

    struct Harness {
        service: Privacy,
        ctx: Ctx<Privacy>,
        state: watch::Receiver<PrivacyState>,
        _inbox: mpsc::Receiver<Input<Privacy>>,
        _cancel: CancellationToken,
    }

    async fn harness(audio: AudioHandle, compositor: CompositorHandle) -> Harness {
        let cancel = CancellationToken::new();
        let (events, inbox) = mpsc::channel(8);
        let (state, state_rx) = watch::channel(PrivacyState::default());
        let (health, _health_rx) = watch::channel(crate::ServiceState::Starting);
        let ctx = Ctx::<Privacy>::new(events, &cancel, state, health, buses());
        let service = Privacy::start(&ctx, NoConfig, Dependencies { audio, compositor })
            .await
            .expect("starts");

        Harness {
            service,
            ctx,
            state: state_rx,
            _inbox: inbox,
            _cancel: cancel,
        }
    }

    #[tokio::test]
    async fn muting_the_microphone_without_a_default_input_is_refused_without_reaching_audio() {
        let cancel = CancellationToken::new();
        let audio = fake_audio(&cancel);
        let compositor = fake_compositor(&cancel);
        let mut harness = harness(audio, compositor).await;

        let (reply, result) = oneshot::channel();
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Command(Command::MuteMicrophone { reply }),
            )
            .await;

        assert!(matches!(
            result.await,
            Ok(Err(PrivacyError::Service(CommandError::Unavailable(_))))
        ));
    }

    #[tokio::test]
    async fn stop_screencast_is_routed_to_the_compositor_handle() {
        let cancel = CancellationToken::new();
        let audio = fake_audio(&cancel);
        let compositor = fake_compositor(&cancel);
        let mut harness = harness(audio, compositor).await;

        let (reply, result) = oneshot::channel();
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Command(Command::StopScreencast {
                    session_id: 2,
                    reply,
                }),
            )
            .await;

        assert!(matches!(
            result.await,
            Ok(Err(PrivacyError::Service(CommandError::Unavailable(_))))
        ));
    }

    fn capturing_app(name: &str) -> AudioState {
        AudioState {
            inputs: vec![AudioDevice {
                id: AudioDeviceId::new("mic"),
                index: 0,
                name: "mic".to_owned(),
                icon_name: None,
                form_factor: None,
                volume: 100,
                muted: false,
                default: true,
            }],
            apps: vec![App {
                id: AppId::new(name),
                name: name.to_owned(),
                icon_name: None,
                app_id: None,
                binary: None,
                playback: None,
                capture: Some(Role {
                    volume: 80,
                    muted: false,
                    adjustable: true,
                    device: AudioDeviceId::new("mic"),
                    corked: false,
                    streams: vec![StreamRef {
                        index: 1,
                        volume: 80,
                        device: AudioDeviceId::new("mic"),
                    }],
                }),
            }],
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn camera_usage_clears_when_the_next_poll_finds_nothing() {
        let cancel = CancellationToken::new();
        let audio = fake_audio(&cancel);
        let compositor = fake_compositor(&cancel);
        let mut harness = harness(audio, compositor).await;

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Camera(vec![fact(Some("ffmpeg"), None, None)])),
            )
            .await;
        assert_eq!(harness.state.borrow_and_update().usages.len(), 1);

        harness
            .service
            .handle(&harness.ctx, Input::Event(Event::Camera(Vec::new())))
            .await;
        assert!(harness.state.borrow_and_update().usages.is_empty());
    }

    #[tokio::test]
    async fn microphone_usage_clears_when_capture_stops() {
        let cancel = CancellationToken::new();
        let audio = fake_audio(&cancel);
        let compositor = fake_compositor(&cancel);
        let mut harness = harness(audio, compositor).await;

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Audio(capturing_app("chrome"))),
            )
            .await;
        assert_eq!(harness.state.borrow_and_update().usages.len(), 1);

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Audio(AudioState::default())),
            )
            .await;
        assert!(harness.state.borrow_and_update().usages.is_empty());
    }

    #[tokio::test]
    async fn location_in_use_becomes_one_usage_and_clears_when_it_ends() {
        let cancel = CancellationToken::new();
        let audio = fake_audio(&cancel);
        let compositor = fake_compositor(&cancel);
        let mut harness = harness(audio, compositor).await;

        harness
            .service
            .handle(&harness.ctx, Input::Event(Event::Location(true)))
            .await;
        let usages = harness.state.borrow_and_update().usages.clone();
        assert_eq!(usages.len(), 1);
        assert_eq!(usages[0].kind, Resource::Location);
        assert_eq!(usages[0].app, None);

        harness
            .service
            .handle(&harness.ctx, Input::Event(Event::Location(false)))
            .await;
        assert!(harness.state.borrow_and_update().usages.is_empty());
    }

    #[tokio::test]
    async fn an_unavailable_event_degrades_health_and_clears_only_its_own_resource() {
        let cancel = CancellationToken::new();
        let audio = fake_audio(&cancel);
        let compositor = fake_compositor(&cancel);
        let mut harness = harness(audio, compositor).await;

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Camera(vec![fact(Some("ffmpeg"), None, None)])),
            )
            .await;
        harness
            .service
            .handle(&harness.ctx, Input::Event(Event::Location(true)))
            .await;
        assert_eq!(harness.state.borrow_and_update().usages.len(), 2);

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Unavailable(
                    Resource::Camera,
                    "camera: /proc/modules is unreadable",
                )),
            )
            .await;

        assert!(
            harness.ctx.is_degraded(),
            "an unavailable source must be visible on health, not just render as nothing in use"
        );
        let usages = harness.state.borrow_and_update().usages.clone();
        assert_eq!(
            usages.len(),
            1,
            "only the resource that became unavailable is cleared"
        );
        assert_eq!(usages[0].kind, Resource::Location);
    }

    fn pipewire_privacy(pw_node_id: u32) -> CompositorPrivacy {
        CompositorPrivacy {
            active: true,
            casts: vec![CastInfo {
                stream_id: 1,
                session_id: Some(11),
                kind: CastKindInfo::PipeWire,
                target: CastTargetInfo::Unknown,
                pw_node_id: Some(pw_node_id),
                active: true,
            }],
        }
    }

    #[tokio::test]
    async fn the_screen_row_renders_before_the_graph_names_anyone() {
        let cancel = CancellationToken::new();
        let audio = fake_audio(&cancel);
        let compositor = fake_compositor(&cancel);
        let mut harness = harness(audio, compositor).await;

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Compositor(Some(pipewire_privacy(107)))),
            )
            .await;

        let usages = harness.state.borrow_and_update().usages.clone();
        assert_eq!(
            usages.len(),
            1,
            "losing or never having the attribution graph must not remove the screen row"
        );
        assert_eq!(usages[0].kind, Resource::Screen);
        assert_eq!(usages[0].app, None);
    }

    #[tokio::test]
    async fn the_graph_names_the_pipewire_cast_once_it_arrives() {
        let cancel = CancellationToken::new();
        let audio = fake_audio(&cancel);
        let compositor = fake_compositor(&cancel);
        let mut harness = harness(audio, compositor).await;

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Compositor(Some(pipewire_privacy(107)))),
            )
            .await;
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Graph(HashMap::from([(107, "chrome".to_owned())]))),
            )
            .await;

        let usages = harness.state.borrow_and_update().usages.clone();
        assert_eq!(usages[0].app.as_deref(), Some("chrome"));
    }

    #[tokio::test]
    async fn a_graph_event_does_not_touch_health() {
        let cancel = CancellationToken::new();
        let audio = fake_audio(&cancel);
        let compositor = fake_compositor(&cancel);
        let mut harness = harness(audio, compositor).await;

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Unavailable(
                    Resource::Camera,
                    "camera: /proc/modules is unreadable",
                )),
            )
            .await;
        assert!(harness.ctx.is_degraded());

        harness
            .service
            .handle(&harness.ctx, Input::Event(Event::Graph(HashMap::new())))
            .await;

        assert!(
            harness.ctx.is_degraded(),
            "losing the attribution graph is not a compositor failure and must not clear an \
             unrelated degradation, nor mark the service running on its own"
        );
    }

    #[tokio::test]
    async fn the_resolved_screen_state_does_not_depend_on_event_order() {
        let privacy = pipewire_privacy(107);
        let attribution = HashMap::from([(107, "chrome".to_owned())]);

        let cancel_a = CancellationToken::new();
        let mut compositor_first = harness(fake_audio(&cancel_a), fake_compositor(&cancel_a)).await;
        compositor_first
            .service
            .handle(
                &compositor_first.ctx,
                Input::Event(Event::Compositor(Some(privacy.clone()))),
            )
            .await;
        compositor_first
            .service
            .handle(
                &compositor_first.ctx,
                Input::Event(Event::Graph(attribution.clone())),
            )
            .await;

        let cancel_b = CancellationToken::new();
        let mut graph_first = harness(fake_audio(&cancel_b), fake_compositor(&cancel_b)).await;
        graph_first
            .service
            .handle(
                &graph_first.ctx,
                Input::Event(Event::Graph(attribution.clone())),
            )
            .await;
        graph_first
            .service
            .handle(
                &graph_first.ctx,
                Input::Event(Event::Compositor(Some(privacy.clone()))),
            )
            .await;

        assert_eq!(
            compositor_first.state.borrow_and_update().usages[0].app,
            graph_first.state.borrow_and_update().usages[0].app,
            "the compositor snapshot and the attribution graph are cached independently, so \
             either arriving first must resolve to the same screen usage"
        );
        assert_eq!(
            graph_first.state.borrow_and_update().usages[0]
                .app
                .as_deref(),
            Some("chrome")
        );
    }
}
