use glimpse_dbus::Buses;
use tokio_util::sync::CancellationToken;

use super::*;

const OWNER: &str = ":1.74";

async fn kdeconnect() -> (
    Kdeconnect,
    Ctx<Kdeconnect>,
    tokio::sync::watch::Receiver<KdeconnectState>,
    tokio::sync::watch::Receiver<crate::ServiceState>,
) {
    let cancel = CancellationToken::new();
    let (events, _inbox) = tokio::sync::mpsc::channel(8);
    let (state, state_rx) = tokio::sync::watch::channel(KdeconnectState::default());
    let (health, health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
    let ctx = Ctx::<Kdeconnect>::new(
        events,
        &cancel,
        state,
        health,
        Buses::unavailable("no bus in tests"),
    );
    let service = Kdeconnect::start(&ctx, NoConfig, ()).await.expect("starts");
    (service, ctx, state_rx, health_rx)
}

fn phone(id: &str, reachable: bool, pair: PairState) -> Device {
    Device::project(
        DeviceId::new(id),
        DeviceProperties {
            name: Some(format!("Phone {id}")),
            kind: DeviceType::Phone,
            reachable,
            pair,
        },
        Some(Battery {
            charge: Some(76),
            charging: false,
            low: false,
        }),
        Actions {
            ring: true,
            ping: true,
            send_clipboard: false,
            share: true,
            browse: true,
            messages: true,
        },
    )
}

fn enumerated(generation: u64, devices: Vec<Device>) -> Event {
    Event::Enumerated {
        generation,
        devices,
    }
}

async fn event(service: &mut Kdeconnect, ctx: &Ctx<Kdeconnect>, event: Event) {
    service.handle(ctx, Input::Event(event)).await;
}

fn keys(service: &Kdeconnect) -> Vec<Watch> {
    service
        .subscriptions()
        .iter()
        .map(|sub| match sub.key() {
            Watch::NameOwner => Watch::NameOwner,
            Watch::Devices(generation) => Watch::Devices(*generation),
        })
        .collect()
}

#[tokio::test]
async fn before_the_first_event_nothing_is_running_and_only_the_owner_is_watched() {
    let (service, _ctx, state, _health) = kdeconnect().await;

    assert_eq!(*state.borrow(), KdeconnectState::default());
    assert_eq!(keys(&service), vec![Watch::NameOwner]);
}

#[tokio::test]
async fn an_owner_starts_the_device_source_and_publishes_running() {
    let (mut service, ctx, state, health) = kdeconnect().await;

    event(&mut service, &ctx, Event::NameOwner(Some(OWNER.to_owned()))).await;

    assert!(state.borrow().running);
    assert_eq!(*health.borrow(), crate::ServiceState::Running);
    assert_eq!(keys(&service), vec![Watch::NameOwner, Watch::Devices(1)]);
}

#[tokio::test]
async fn no_owner_is_healthy_and_publishes_nothing_running() {
    let (mut service, ctx, state, health) = kdeconnect().await;

    event(&mut service, &ctx, Event::NameOwner(Some(OWNER.to_owned()))).await;
    event(
        &mut service,
        &ctx,
        enumerated(1, vec![phone("a", true, PairState::Paired)]),
    )
    .await;
    event(&mut service, &ctx, Event::NameOwner(None)).await;

    assert_eq!(*state.borrow(), KdeconnectState::default());
    assert_eq!(
        *health.borrow(),
        crate::ServiceState::Running,
        "no daemon is the normal state for most users, not a fault"
    );
    assert_eq!(keys(&service), vec![Watch::NameOwner]);
}

#[tokio::test]
async fn a_restarted_daemon_is_a_new_generation() {
    let (mut service, ctx, _state, _health) = kdeconnect().await;

    event(&mut service, &ctx, Event::NameOwner(Some(OWNER.to_owned()))).await;
    event(
        &mut service,
        &ctx,
        Event::NameOwner(Some(":1.99".to_owned())),
    )
    .await;

    assert_eq!(keys(&service), vec![Watch::NameOwner, Watch::Devices(2)]);
}

#[tokio::test]
async fn the_same_owner_twice_does_not_restart_the_source() {
    let (mut service, ctx, _state, _health) = kdeconnect().await;

    event(&mut service, &ctx, Event::NameOwner(Some(OWNER.to_owned()))).await;
    event(&mut service, &ctx, Event::NameOwner(Some(OWNER.to_owned()))).await;

    assert_eq!(keys(&service), vec![Watch::NameOwner, Watch::Devices(1)]);
}

#[tokio::test]
async fn a_device_signal_replaces_inserts_and_removes_by_id() {
    let (mut service, ctx, state, _health) = kdeconnect().await;
    event(&mut service, &ctx, Event::NameOwner(Some(OWNER.to_owned()))).await;
    event(
        &mut service,
        &ctx,
        enumerated(
            1,
            vec![
                phone("b", true, PairState::Paired),
                phone("a", false, PairState::Paired),
            ],
        ),
    )
    .await;
    let ids = |state: &KdeconnectState| {
        state
            .devices
            .iter()
            .map(|device| device.id.as_str().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&state.borrow()), ["a", "b"]);

    event(
        &mut service,
        &ctx,
        Event::Device {
            generation: 1,
            id: DeviceId::new("a"),
            device: Some(phone("a", true, PairState::Paired)),
        },
    )
    .await;
    assert!(state.borrow().devices[0].reachable);

    event(
        &mut service,
        &ctx,
        Event::Device {
            generation: 1,
            id: DeviceId::new("c"),
            device: Some(phone("c", true, PairState::NotPaired)),
        },
    )
    .await;
    assert_eq!(ids(&state.borrow()), ["a", "b", "c"]);

    event(
        &mut service,
        &ctx,
        Event::Device {
            generation: 1,
            id: DeviceId::new("b"),
            device: None,
        },
    )
    .await;
    assert_eq!(ids(&state.borrow()), ["a", "c"]);
}

#[tokio::test]
async fn a_dead_daemons_late_list_cannot_resurrect_its_devices() {
    let (mut service, ctx, state, health) = kdeconnect().await;
    event(&mut service, &ctx, Event::NameOwner(Some(OWNER.to_owned()))).await;
    event(&mut service, &ctx, Event::NameOwner(None)).await;

    event(
        &mut service,
        &ctx,
        enumerated(1, vec![phone("a", true, PairState::Paired)]),
    )
    .await;
    event(
        &mut service,
        &ctx,
        Event::Failed {
            generation: 1,
            reason: "gone".to_owned(),
        },
    )
    .await;

    assert_eq!(*state.borrow(), KdeconnectState::default());
    assert_eq!(*health.borrow(), crate::ServiceState::Running);
}

#[tokio::test]
async fn a_burst_of_signals_is_one_fetch_per_device_and_one_list_at_a_time() {
    let (mut service, _ctx, _state, _health) = kdeconnect().await;
    let phone = || Stale::Device(DeviceId::new("a"));

    assert_eq!(service.plan(phone()), Some(phone()));
    for _ in 0..100 {
        assert_eq!(service.plan(phone()), None, "a fetch is already in flight");
    }
    assert!(service.fetches.refetch.contains(&DeviceId::new("a")));

    assert_eq!(service.plan(Stale::All), Some(Stale::All));
    assert_eq!(service.plan(Stale::All), None);
    assert!(
        service.fetches.relist,
        "the second list waits for the first"
    );
}

#[test]
fn only_a_paired_reachable_device_carries_a_battery_and_actions() {
    let connected = phone("a", true, PairState::Paired);
    assert!(connected.battery.is_some());
    assert!(connected.actions.ring);

    for device in [
        phone("b", false, PairState::Paired),
        phone("c", true, PairState::NotPaired),
        phone("d", true, PairState::Requested),
    ] {
        assert_eq!(device.battery, None, "{}", device.id.as_str());
        assert_eq!(device.actions, Actions::default(), "{}", device.id.as_str());
    }
}

#[test]
fn only_a_connected_device_with_a_reading_contributes_a_battery() {
    let mut unread = phone("c", true, PairState::Paired);
    unread.battery = Some(Battery {
        charge: None,
        charging: true,
        low: false,
    });
    let mut low = phone("a", true, PairState::Paired);
    low.battery = Some(Battery {
        charge: Some(9),
        charging: false,
        low: true,
    });
    let contributed = batteries(&KdeconnectState {
        running: true,
        devices: vec![low, phone("b", false, PairState::Paired), unread],
    });
    assert_eq!(contributed.len(), 1);
    assert_eq!(contributed[0].path, "kdeconnect:a");
    assert_eq!(contributed[0].name, "Phone a");
    assert_eq!(contributed[0].kind, DeviceKind::Phone);
    assert_eq!(contributed[0].charge.percentage, 9);
    assert_eq!(contributed[0].charge.state, ChargeState::Discharging);
    assert_eq!(contributed[0].charge.warning, WarningLevel::Low);
}

#[test]
fn a_nameless_device_falls_back_to_its_id() {
    let device = Device::project(
        DeviceId::new("b98d"),
        DeviceProperties::default(),
        None,
        Actions::default(),
    );
    assert_eq!(device.name, "b98d");
}

#[tokio::test]
async fn a_command_with_no_daemon_is_refused_as_unavailable() {
    let (mut service, ctx, _state, _health) = kdeconnect().await;
    let (reply, result) = oneshot::channel();

    service
        .handle(
            &ctx,
            Input::Command(Command::Act {
                id: DeviceId::new("a"),
                action: Action::Ring,
                reply,
            }),
        )
        .await;

    assert!(matches!(
        result.await.expect("a reply"),
        Err(CommandError::Unavailable(_))
    ));
}
