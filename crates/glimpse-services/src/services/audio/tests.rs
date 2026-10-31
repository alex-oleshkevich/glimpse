use crate::CommandError;
use glimpse_dbus::Buses;
use tokio::sync::{mpsc, watch};
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
    let service = Audio::start(&ctx, NoConfig, ()).await;
    (service, ctx, state_rx, health_rx)
}

fn device(id: &str, index: u32, default: bool) -> Device {
    Device {
        id: DeviceId::new(id),
        index,
        name: id.to_owned(),
        port: None,
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
                .map(|(index, volume)| StreamRef {
                    index,
                    volume,
                    device: DeviceId::new("headset"),
                })
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

async fn ready(service: &mut Audio, ctx: &Ctx<Audio>) -> mpsc::Receiver<pulse::Request> {
    let (tx, rx) = mpsc::channel(1);
    service
        .handle(ctx, Input::Event(pulse::Event::Ready(pulse::Client { tx })))
        .await;
    rx
}

#[tokio::test]
async fn ready_stores_the_client_and_a_subsequent_command_is_not_unavailable() {
    let (mut service, ctx, _state, health) = harness().await;
    let mut rx = ready(&mut service, &ctx).await;

    assert!(service.client.is_some());
    assert!(matches!(&*health.borrow(), ServiceState::Running));

    service.current = AudioState {
        outputs: vec![device("headset", 1, true)],
        inputs: Vec::new(),
        apps: Vec::new(),
    };

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

    let request = rx.recv().await.expect("the client forwards the command");
    let pulse::Request::SetDeviceVolume {
        reply: req_reply, ..
    } = request
    else {
        panic!("expected a SetDeviceVolume request");
    };
    let _ = req_reply.send(Ok(()));

    assert!(!matches!(result.await, Ok(Err(AudioError::Unavailable))));
}

#[tokio::test]
async fn set_app_volume_sends_the_scaled_fan_out_as_one_set_stream_volume_request() {
    let (mut service, ctx, _state, _health) = harness().await;
    let mut rx = ready(&mut service, &ctx).await;
    service.current = AudioState {
        outputs: Vec::new(),
        inputs: Vec::new(),
        apps: vec![app("firefox", vec![(1, 20), (2, 80)])],
    };

    let (reply, result) = oneshot::channel();
    service
        .handle(
            &ctx,
            Input::Command(Command::SetAppVolume {
                dir: Direction::Output,
                app: AppId::new("firefox"),
                percent: 40,
                reply,
            }),
        )
        .await;

    let request = rx.recv().await.expect("the client forwards the command");
    let pulse::Request::SetStreamVolume {
        dir,
        streams,
        reply: req_reply,
    } = request
    else {
        panic!("expected a SetStreamVolume request");
    };
    assert_eq!(dir, Direction::Output);
    assert_eq!(streams, vec![(1, 10), (2, 40)]);
    let _ = req_reply.send(Ok(()));
    let _ = result.await;
}

#[tokio::test]
async fn set_device_muted_dispatches_a_set_device_muted_request() {
    let (mut service, ctx, _state, _health) = harness().await;
    let mut rx = ready(&mut service, &ctx).await;
    service.current = AudioState {
        outputs: vec![device("headset", 1, true)],
        inputs: Vec::new(),
        apps: Vec::new(),
    };

    let (reply, result) = oneshot::channel();
    service
        .handle(
            &ctx,
            Input::Command(Command::SetDeviceMuted {
                dir: Direction::Output,
                id: DeviceId::new("headset"),
                muted: true,
                reply,
            }),
        )
        .await;

    let request = rx.recv().await.expect("the client forwards the command");
    let pulse::Request::SetDeviceMuted {
        dir,
        index,
        muted,
        reply: req_reply,
    } = request
    else {
        panic!("expected a SetDeviceMuted request");
    };
    assert_eq!(dir, Direction::Output);
    assert_eq!(index, 1);
    assert!(muted);
    let _ = req_reply.send(Ok(()));
    let _ = result.await;
}

#[tokio::test]
async fn set_default_dispatches_a_set_default_request() {
    let (mut service, ctx, _state, _health) = harness().await;
    let mut rx = ready(&mut service, &ctx).await;
    service.current = AudioState {
        outputs: vec![device("headset", 1, true)],
        inputs: Vec::new(),
        apps: Vec::new(),
    };

    let (reply, result) = oneshot::channel();
    service
        .handle(
            &ctx,
            Input::Command(Command::SetDefault {
                dir: Direction::Output,
                id: DeviceId::new("headset"),
                reply,
            }),
        )
        .await;

    let request = rx.recv().await.expect("the client forwards the command");
    let pulse::Request::SetDefault {
        dir,
        name,
        reply: req_reply,
    } = request
    else {
        panic!("expected a SetDefault request");
    };
    assert_eq!(dir, Direction::Output);
    assert_eq!(name, "headset");
    let _ = req_reply.send(Ok(()));
    let _ = result.await;
}

#[tokio::test]
async fn set_app_muted_dispatches_a_set_stream_muted_request_for_every_stream() {
    let (mut service, ctx, _state, _health) = harness().await;
    let mut rx = ready(&mut service, &ctx).await;
    service.current = AudioState {
        outputs: Vec::new(),
        inputs: Vec::new(),
        apps: vec![app("firefox", vec![(1, 20), (2, 80)])],
    };

    let (reply, result) = oneshot::channel();
    service
        .handle(
            &ctx,
            Input::Command(Command::SetAppMuted {
                dir: Direction::Output,
                app: AppId::new("firefox"),
                muted: true,
                reply,
            }),
        )
        .await;

    let request = rx.recv().await.expect("the client forwards the command");
    let pulse::Request::SetStreamMuted {
        dir,
        streams,
        muted,
        reply: req_reply,
    } = request
    else {
        panic!("expected a SetStreamMuted request");
    };
    assert_eq!(dir, Direction::Output);
    assert_eq!(streams, vec![1, 2]);
    assert!(muted);
    let _ = req_reply.send(Ok(()));
    let _ = result.await;
}

#[tokio::test]
async fn move_app_dispatches_a_move_streams_request_to_the_target_device() {
    let (mut service, ctx, _state, _health) = harness().await;
    let mut rx = ready(&mut service, &ctx).await;
    service.current = AudioState {
        outputs: vec![device("hdmi", 3, false)],
        inputs: Vec::new(),
        apps: vec![app("firefox", vec![(1, 20), (2, 80)])],
    };

    let (reply, result) = oneshot::channel();
    service
        .handle(
            &ctx,
            Input::Command(Command::MoveApp {
                dir: Direction::Output,
                app: AppId::new("firefox"),
                to: DeviceId::new("hdmi"),
                reply,
            }),
        )
        .await;

    let request = rx.recv().await.expect("the client forwards the command");
    let pulse::Request::MoveStreams {
        dir,
        streams,
        target,
        reply: req_reply,
    } = request
    else {
        panic!("expected a MoveStreams request");
    };
    assert_eq!(dir, Direction::Output);
    assert_eq!(streams, vec![1, 2]);
    assert_eq!(target, 3);
    let _ = req_reply.send(Ok(()));
    let _ = result.await;
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

async fn gone(service: &mut Audio, ctx: &Ctx<Audio>) {
    service
        .handle(ctx, Input::Event(pulse::Event::Gone("down".to_owned())))
        .await;
}

#[tokio::test]
async fn reconnecting_waits_longer_after_each_failure_up_to_a_cap() {
    let (mut service, ctx, _state, _health) = harness().await;
    assert_eq!(service.backoff, Duration::ZERO);

    let mut waits = Vec::new();
    for _ in 0..8 {
        gone(&mut service, &ctx).await;
        waits.push(service.backoff.as_secs());
    }

    assert_eq!(waits, [1, 2, 4, 8, 16, 30, 30, 30]);
}

#[tokio::test]
async fn a_ready_connection_resets_the_backoff() {
    let (mut service, ctx, _state, _health) = harness().await;
    gone(&mut service, &ctx).await;
    gone(&mut service, &ctx).await;
    assert_eq!(service.backoff, Duration::from_secs(2));

    let (tx, _requests) = mpsc::channel(1);
    service
        .handle(
            &ctx,
            Input::Event(pulse::Event::Ready(pulse::Client { tx })),
        )
        .await;
    assert_eq!(service.backoff, Duration::ZERO);

    gone(&mut service, &ctx).await;
    assert_eq!(service.backoff, Duration::from_secs(1));
}

#[tokio::test]
async fn volume_steps_reach_pulse_in_the_order_they_were_issued() {
    let (mut service, ctx, _state, _health) = harness().await;
    let (tx, mut requests) = mpsc::channel(8);
    service
        .handle(
            &ctx,
            Input::Event(pulse::Event::Ready(pulse::Client { tx })),
        )
        .await;
    service
        .handle(
            &ctx,
            Input::Event(pulse::Event::Snapshot(Box::new(snapshot(
                vec![device("headset", 1, true)],
                Vec::new(),
            )))),
        )
        .await;

    for percent in [10, 20, 30] {
        let (reply, _result) = oneshot::channel();
        service
            .handle(
                &ctx,
                Input::Command(Command::SetDeviceVolume {
                    dir: Direction::Output,
                    id: DeviceId::new("headset"),
                    percent,
                    reply,
                }),
            )
            .await;
    }

    let mut seen = Vec::new();
    while let Ok(pulse::Request::SetDeviceVolume { percent, .. }) = requests.try_recv() {
        seen.push(percent);
    }
    assert_eq!(seen, [10, 20, 30]);
}
