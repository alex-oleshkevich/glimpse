use glimpse_dbus::Buses;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::service::{NoConfig, ServiceRuntime, ServiceState};

async fn harness() -> (
    Audio,
    Ctx<Audio>,
    watch::Receiver<AudioState>,
    watch::Receiver<ServiceState>,
) {
    let cancel = CancellationToken::new();
    let (events, _inbox) = tokio::sync::mpsc::channel(8);
    let (state, state_rx) = watch::channel(AudioState::default());
    let (health, health_rx) = watch::channel(ServiceState::Starting);
    let ctx = Ctx::<Audio>::new(
        events,
        &cancel,
        state,
        health,
        Buses::unavailable("no bus in tests"),
    );
    let service = Audio::start(&ctx, NoConfig, ())
        .await
        .expect("starts with no config and no dependencies");
    (service, ctx, state_rx, health_rx)
}

fn device(id: &str, index: u32, default: bool) -> Device {
    Device {
        id: DeviceId::new(id),
        index,
        name: id.to_owned(),
        icon_name: None,
        form_factor: None,
        volume: 100,
        muted: false,
        default,
    }
}

fn app(id: &str, streams: Vec<(u32, u32)>) -> App {
    App {
        id: AppId::new(id),
        name: id.to_owned(),
        icon_name: None,
        app_id: None,
        binary: None,
        playback: Some(Role {
            volume: streams.iter().map(|(_, volume)| *volume).max().unwrap_or(0),
            muted: false,
            adjustable: true,
            device: DeviceId::new("headset"),
            corked: false,
            streams: streams
                .into_iter()
                .map(|(index, volume)| StreamRef { index, volume })
                .collect(),
        }),
        capture: None,
    }
}

fn snapshot(outputs: Vec<Device>, apps: Vec<App>) -> pulse::Snapshot {
    pulse::Snapshot {
        outputs,
        inputs: Vec::new(),
        apps,
    }
}

fn pulse_generation(service: &Audio) -> u64 {
    let subs = service.subscriptions();
    let Watch::Pulse(generation) = subs.first().expect("audio declares one pulse source").key();
    *generation
}

#[tokio::test]
async fn start_builds_the_model_with_no_client_and_generation_zero() {
    let (service, _ctx, _state, _health) = harness().await;

    assert!(service.client.is_none());
    assert_eq!(pulse_generation(&service), 0);
}

#[tokio::test]
async fn a_snapshot_publishes_the_state() {
    let (mut service, ctx, mut state, _health) = harness().await;

    service
        .handle(
            &ctx,
            Input::Event(pulse::Event::Snapshot(Box::new(snapshot(
                vec![device("headset", 1, true)],
                Vec::new(),
            )))),
        )
        .await;

    assert!(state.has_changed().expect("sender is alive"));
    assert_eq!(state.borrow_and_update().outputs.len(), 1);
}

#[tokio::test]
async fn an_identical_snapshot_is_not_republished() {
    let (mut service, ctx, mut state, _health) = harness().await;
    let one = || snapshot(vec![device("headset", 1, true)], Vec::new());

    service
        .handle(&ctx, Input::Event(pulse::Event::Snapshot(Box::new(one()))))
        .await;
    state.borrow_and_update();

    service
        .handle(&ctx, Input::Event(pulse::Event::Snapshot(Box::new(one()))))
        .await;

    assert!(
        !state.has_changed().unwrap_or(false),
        "an identical snapshot must not reach a subscriber a second time"
    );
}

#[tokio::test]
async fn gone_degrades_and_bumps_the_generation_so_the_subscription_key_changes() {
    let (mut service, ctx, _state, health) = harness().await;
    let before = pulse_generation(&service);

    service
        .handle(
            &ctx,
            Input::Event(pulse::Event::Gone(
                "the pulse connection was lost".to_owned(),
            )),
        )
        .await;

    assert!(matches!(&*health.borrow(), ServiceState::Degraded { .. }));
    assert!(service.client.is_none());
    assert_ne!(
        before,
        pulse_generation(&service),
        "a dead source is never restarted unless its key actually changes"
    );
}

/// `run` checks the client before resolving an id, so a full dispatch only ever answers
/// `Refused` once a client exists — unconstructible headlessly. This exercises the resolver
/// `run` itself calls for that answer.
#[tokio::test]
async fn device_index_is_none_for_a_device_absent_from_the_current_snapshot() {
    let (mut service, _ctx, _state, _health) = harness().await;
    service.current = AudioState {
        outputs: vec![device("headset", 1, true)],
        inputs: Vec::new(),
        apps: Vec::new(),
    };

    assert_eq!(
        service.device_index(Direction::Output, &DeviceId::new("ghost")),
        None
    );
}

#[tokio::test]
async fn app_role_is_none_for_an_app_absent_from_the_current_snapshot() {
    let (mut service, _ctx, _state, _health) = harness().await;
    service.current = AudioState {
        outputs: Vec::new(),
        inputs: Vec::new(),
        apps: vec![app("firefox", vec![(10, 50)])],
    };

    assert!(
        service
            .app_role(Direction::Output, &AppId::new("ghost"))
            .is_none()
    );
}

/// The window this guards: before the very first `pulse::Event` of any kind arrives,
/// `current` is `AudioState::default()` and `client` is `None`. A command issued in exactly
/// that window must read as "audio is not connected yet", not as "no such device" — the id
/// resolver would find nothing in an empty snapshot either way, so the client must be checked
/// first or a disconnection hides behind a no-such-device message.
#[tokio::test]
async fn a_command_before_any_event_has_arrived_is_unavailable_not_refused() {
    let (mut service, ctx, _state, _health) = harness().await;

    let (reply, result) = oneshot::channel();
    service
        .handle(
            &ctx,
            Input::Command(Command::SetDeviceVolume {
                dir: Direction::Output,
                id: DeviceId::new("headset"),
                percent: 50,
                reply,
            }),
        )
        .await;

    assert!(matches!(result.await, Ok(Err(AudioError::Unavailable))));
}

#[tokio::test]
async fn a_command_naming_a_known_device_while_the_client_is_none_is_unavailable() {
    let (mut service, ctx, _state, _health) = harness().await;
    service
        .handle(
            &ctx,
            Input::Event(pulse::Event::Snapshot(Box::new(snapshot(
                vec![device("headset", 1, true)],
                Vec::new(),
            )))),
        )
        .await;
    assert!(service.client.is_none());

    let (reply, result) = oneshot::channel();
    service
        .handle(
            &ctx,
            Input::Command(Command::SetDeviceVolume {
                dir: Direction::Output,
                id: DeviceId::new("headset"),
                percent: 50,
                reply,
            }),
        )
        .await;

    assert!(matches!(result.await, Ok(Err(AudioError::Unavailable))));
}

#[tokio::test]
async fn set_app_volume_scales_each_stream_against_the_group_max() {
    let (mut service, _ctx, _state, _health) = harness().await;
    service.current = AudioState {
        outputs: Vec::new(),
        inputs: Vec::new(),
        apps: vec![app("firefox", vec![(1, 20), (2, 80)])],
    };

    let role = service
        .app_role(Direction::Output, &AppId::new("firefox"))
        .expect("the fixture carries a playback role for firefox");

    assert_eq!(
        role.scaled(40),
        vec![(1, 10), (2, 40)],
        "two streams at 20% and 80% set to 40% become 10% and 40%, not 40% and 40%"
    );
}

#[tokio::test]
async fn a_stopped_service_maps_a_dropped_reply_to_unavailable() {
    let cancel = CancellationToken::new();
    let (runtime, handle) =
        ServiceRuntime::<Audio>::new(NoConfig, Buses::unavailable("no bus in tests"), cancel);
    drop(runtime);

    let result = handle
        .set_device_volume(Direction::Output, DeviceId::new("headset"), 50)
        .await;

    assert!(matches!(
        result,
        Err(AudioError::Service(CommandError::Unavailable(_)))
    ));
}
