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
    let (runtime, handle) = ServiceRuntime::<Compositor>::new(NoConfig, buses(), cancel.clone());
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
    let service = Privacy::start(&ctx, NoConfig, Dependencies { audio, compositor }).await;

    Harness {
        service,
        ctx,
        state: state_rx,
        _inbox: inbox,
        _cancel: cancel,
    }
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
