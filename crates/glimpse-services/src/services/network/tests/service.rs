use super::*;
use crate::service::ServiceState;
use glimpse_dbus::Buses;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub(in crate::services::network) struct Harness {
    pub(in crate::services::network) service: Network,
    pub(in crate::services::network) ctx: Ctx<Network>,
    pub(in crate::services::network) state: tokio::sync::watch::Receiver<NetworkState>,
    pub(in crate::services::network) health: tokio::sync::watch::Receiver<ServiceState>,
    _inbox: mpsc::Receiver<Input<Network>>,
    _cancel: CancellationToken,
}

pub(in crate::services::network) async fn harness() -> Harness {
    started(Dependencies { agent: true }).await
}

pub(in crate::services::network) async fn harness_without_agent() -> Harness {
    started(Dependencies { agent: false }).await
}

async fn started(dependencies: Dependencies) -> Harness {
    let (events, inbox) = mpsc::channel(32);
    let cancel = CancellationToken::new();
    let config = Config::from(&glimpse_config::Config::default());
    let (health, health_rx) = tokio::sync::watch::channel(ServiceState::Starting);
    let (state, state_rx) =
        tokio::sync::watch::channel(<Network as Service>::initial_state(&config));
    let ctx = Ctx::<Network>::new(
        events,
        &cancel,
        state,
        health,
        Buses::unavailable("no bus in tests"),
    );
    let service = Network::start(&ctx, config, dependencies).await;

    Harness {
        service,
        ctx,
        state: state_rx,
        health: health_rx,
        _inbox: inbox,
        _cancel: cancel,
    }
}

fn manager_object(wifi_on: bool, hardware: bool) -> (String, Interfaces) {
    (
        nm::MANAGER.to_owned(),
        HashMap::from([(
            nm::MANAGER1.to_owned(),
            properties(vec![
                ("NetworkingEnabled", owned(true)),
                ("WirelessEnabled", owned(wifi_on)),
                ("WirelessHardwareEnabled", owned(hardware)),
                ("Connectivity", owned(4u32)),
                ("Metered", owned(4u32)),
            ]),
        )]),
    )
}

fn device_object(path: &str, kind: u32, managed: bool) -> (String, Interfaces) {
    (
        path.to_owned(),
        HashMap::from([(
            nm::DEVICE1.to_owned(),
            properties(vec![
                ("DeviceType", owned(kind)),
                ("Managed", owned(managed)),
                ("State", owned(100u32)),
                ("Interface", owned("x")),
            ]),
        )]),
    )
}

pub(in crate::services::network) fn measured_objects() -> Objects {
    Objects::from([
        manager_object(true, true),
        device_object("/org/freedesktop/NetworkManager/Devices/2", 2, true),
        device_object("/org/freedesktop/NetworkManager/Devices/1", 32, true),
        device_object("/org/freedesktop/NetworkManager/Devices/4", 13, true),
        device_object("/org/freedesktop/NetworkManager/Devices/5", 13, true),
        device_object("/org/freedesktop/NetworkManager/Devices/241", 13, true),
        device_object("/org/freedesktop/NetworkManager/Devices/242", 20, false),
        device_object("/org/freedesktop/NetworkManager/Devices/314", 20, false),
        device_object("/org/freedesktop/NetworkManager/Devices/305", 30, true),
        device_object("/org/freedesktop/NetworkManager/Devices/367", 5, true),
    ])
}

pub(in crate::services::network) async fn enumerate(harness: &mut Harness, objects: Objects) {
    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Enumerated(Box::new(objects))),
        )
        .await;
}

#[tokio::test]
async fn enumerating_the_measured_bus_keeps_one_device_and_drops_the_other_eight() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;

    let state = harness.state.borrow_and_update().clone();
    assert!(
        state.wifi.is_some(),
        "wlp99s0 is the one user-facing device"
    );
    assert!(state.wired.is_empty(), "there is no ethernet device here");
    assert!(
        state.networks.is_empty(),
        "p2p-dev-wlp99s0 is a device and never a network anyone can join"
    );
    assert_eq!(*harness.health.borrow_and_update(), ServiceState::Running);
}

#[tokio::test]
async fn a_radio_write_the_backend_never_took_is_not_published_as_if_it_had() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;
    assert!(
        harness
            .state
            .borrow_and_update()
            .wifi
            .is_some_and(|radio| radio.enabled)
    );

    let (reply, result) = tokio::sync::oneshot::channel();
    harness
        .service
        .handle(
            &harness.ctx,
            Input::Command(Command::SetWifiEnabled {
                enabled: false,
                reply,
            }),
        )
        .await;
    assert!(matches!(
        result.await.expect("the sender was not dropped"),
        Err(NetworkError::Unavailable(_))
    ));

    assert!(
        harness
            .state
            .borrow_and_update()
            .wifi
            .is_some_and(|radio| radio.enabled),
        "a refused write emits no property change, so publishing it first leaves the switch \
         disagreeing with NetworkManager until something else moves"
    );
}

#[tokio::test]
async fn a_command_with_no_bus_is_refused_rather_than_queued() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;

    let (reply, result) = tokio::sync::oneshot::channel();
    harness
        .service
        .handle(&harness.ctx, Input::Command(Command::StartScan { reply }))
        .await;

    let outcome = tokio::time::timeout(std::time::Duration::from_secs(1), result)
        .await
        .expect("the reply arrives rather than hanging")
        .expect("the sender was not dropped");
    assert!(
        matches!(outcome, Err(NetworkError::Unavailable(_))),
        "a scan with no bus must answer, not wait for one"
    );
}

#[tokio::test]
async fn networkmanager_leaving_the_bus_degrades_and_clears_what_it_told_us() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;

    harness
        .service
        .handle(&harness.ctx, Input::Event(Event::NameOwner(None)))
        .await;

    assert!(matches!(
        *harness.health.borrow_and_update(),
        ServiceState::Degraded { .. }
    ));
    assert!(harness.state.borrow_and_update().wifi.is_none());
}

#[tokio::test]
async fn networkmanager_coming_back_bumps_the_generation_so_every_source_restarts() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;
    let before = harness.service.generation;

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::NameOwner(Some(":1.42".to_owned()))),
        )
        .await;

    assert_ne!(harness.service.generation, before);
    let keys: Vec<Watch> = harness
        .service
        .subscriptions()
        .into_iter()
        .map(|sub| match sub.key() {
            Watch::NameOwner => Watch::NameOwner,
            Watch::Objects(n) => Watch::Objects(*n),
            Watch::Properties(n) => Watch::Properties(*n),
            Watch::ActiveStates(n) => Watch::ActiveStates(*n),
            Watch::DeviceStates(n) => Watch::DeviceStates(*n),
            Watch::ProfileUpdates(n) => Watch::ProfileUpdates(*n),
            Watch::ScanDeadline(n) => Watch::ScanDeadline(*n),
        })
        .collect();
    assert!(
        keys.contains(&Watch::Objects(harness.service.generation)),
        "the object source is keyed on the new generation, so the runtime restarts it"
    );
    assert!(
        keys.contains(&Watch::ProfileUpdates(harness.service.generation)),
        "a profile edited in place announces itself only through Settings.Connection.Updated,              so that source has to be declared and to restart with the generation"
    );
}

#[tokio::test]
async fn mirror_only_mode_never_enables_agent_registration() {
    let mut harness = harness_without_agent().await;
    assert_eq!(
        harness.service.register_attempts.get(),
        0,
        "a locked screen must never register a secret agent"
    );

    enumerate(&mut harness, measured_objects()).await;
    assert_eq!(harness.service.register_attempts.get(), 0);

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::NameOwner(Some(":1.7".to_owned()))),
        )
        .await;
    assert_eq!(
        harness.service.register_attempts.get(),
        0,
        "coming back onto the bus must not attempt registration in mirror-only mode"
    );
}

#[tokio::test]
async fn the_panel_path_still_attempts_agent_registration() {
    let mut harness = harness().await;
    assert_eq!(
        harness.service.register_attempts.get(),
        0,
        "nothing attempts registration before the first enumeration"
    );

    enumerate(&mut harness, measured_objects()).await;
    assert!(
        harness.service.register_attempts.get() > 0,
        "enumerating with the agent enabled must attempt registration, same as today"
    );

    let after_enumerate = harness.service.register_attempts.get();
    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::NameOwner(Some(":1.7".to_owned()))),
        )
        .await;
    assert!(
        harness.service.register_attempts.get() > after_enumerate,
        "the bus reappearing re-attempts registration when the agent is enabled"
    );
}

fn activated(path: &str, settings: &str, device: &str) -> nm::ActiveProperties {
    nm::ActiveProperties {
        id: Some(format!("active {path}")),
        state: nm::ActiveState::Activated,
        devices: vec![device.to_owned()],
        connection: Some(settings.to_owned()),
        ..Default::default()
    }
}

#[tokio::test]
async fn a_failure_is_filed_under_the_name_the_beacon_row_looks_it_up_by() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;
    let path = "/org/freedesktop/NetworkManager/ActiveConnection/16";
    let settings = "/org/freedesktop/NetworkManager/Settings/5";

    let mut named = nm::Profile {
        ssid: Some("Skylink".to_owned()),
        ..nm::Profile::default()
    };
    named.id = Some("Home Wi-Fi".to_owned());
    harness.service.profiles.insert(settings.to_owned(), named);
    harness
        .service
        .actives
        .insert(path.to_owned(), activated(path, settings, "/d/1"));

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::ActiveStateChanged {
                path: path.to_owned(),
                state: nm::ActiveState::Deactivated,
                reason: 10,
            }),
        )
        .await;

    assert!(
        harness.service.failures.contains_key("Skylink"),
        "a profile named for anything but its SSID would file the failure where no row reads it"
    );
    assert!(!harness.service.failures.contains_key("Home Wi-Fi"));
}

#[tokio::test]
async fn a_neutral_reason_at_teardown_falls_back_to_the_one_that_arrived_before_it() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;
    let path = "/org/freedesktop/NetworkManager/ActiveConnection/16";
    let settings = "/org/freedesktop/NetworkManager/Settings/5";

    harness.service.profiles.insert(
        settings.to_owned(),
        nm::Profile {
            ssid: Some("Skylink".to_owned()),
            ..nm::Profile::default()
        },
    );
    harness
        .service
        .actives
        .insert(path.to_owned(), activated(path, settings, "/d/1"));

    for (state, reason) in [
        (nm::ActiveState::Activating, 10u32),
        (nm::ActiveState::Deactivated, 1),
    ] {
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::ActiveStateChanged {
                    path: path.to_owned(),
                    state,
                    reason,
                }),
            )
            .await;
    }

    assert!(
        harness.service.failures.contains_key("Skylink"),
        "NetworkManager carries the useful reason before state 4 and a neutral one with it; \
         without the cache the failure is dropped on the floor"
    );
}

#[tokio::test]
async fn commands_go_through_the_adapter_that_is_carrying_the_connection() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;
    harness.service.devices.clear();

    let idle = "/org/freedesktop/NetworkManager/Devices/1";
    let carrying = "/org/freedesktop/NetworkManager/Devices/9";
    for (path, state) in [
        (idle, nm::DeviceState::Disconnected),
        (carrying, nm::DeviceState::Activated),
    ] {
        let mut record = DeviceRecord {
            properties: device(2, true),
            ..Default::default()
        };
        record.properties.state = state;
        harness.service.devices.insert(path.to_owned(), record);
    }

    assert_eq!(
        harness
            .service
            .wireless_device()
            .map(|(path, _)| path.clone()),
        Some(carrying.to_owned()),
        "the lowest object path is not the adapter the user is on"
    );
}

#[tokio::test]
async fn an_active_connection_answers_for_the_device_it_runs_on() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;
    let path = "/org/freedesktop/NetworkManager/ActiveConnection/16";
    let device = "/org/freedesktop/NetworkManager/Devices/7";

    harness
        .service
        .actives
        .insert(path.to_owned(), activated(path, "/s/1", device));

    assert_eq!(
        harness
            .service
            .active_for(&NetworkId::new(device.to_owned())),
        Some(path.to_owned()),
        "a wired row carries a device path, and disconnecting it must find the connection"
    );
}

#[tokio::test]
async fn a_recycled_active_connection_path_never_serves_a_stale_reason() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;
    let path = "/org/freedesktop/NetworkManager/ActiveConnection/16";

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::ActiveStateChanged {
                path: path.to_owned(),
                state: nm::ActiveState::Activating,
                reason: 10,
            }),
        )
        .await;
    assert_eq!(harness.service.reasons.get(path), Some(&10));

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::ActiveStateChanged {
                path: path.to_owned(),
                state: nm::ActiveState::Deactivated,
                reason: 2,
            }),
        )
        .await;
    assert_eq!(
        harness.service.reasons.get(path),
        None,
        "Connection.Active has no StateReason property, so a stale cache entry is the bug"
    );
}

#[tokio::test]
async fn a_statistics_change_does_not_wake_the_model() {
    let mut harness = harness().await;
    enumerate(&mut harness, measured_objects()).await;
    let before = harness.state.borrow_and_update().clone();

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::PropertiesChanged {
                path: "/org/freedesktop/NetworkManager/Devices/2".to_owned(),
                interface: "org.freedesktop.NetworkManager.Device.Statistics".to_owned(),
                changed: properties(vec![
                    ("RxBytes", owned(29540411578u64)),
                    ("TxBytes", owned(20477124132u64)),
                ]),
                invalidated: Vec::new(),
            }),
        )
        .await;

    assert_eq!(
        harness.state.borrow_and_update().clone(),
        before,
        "Bitrate and the statistics counters churn and feed nothing"
    );
}

#[tokio::test]
async fn the_radio_reports_hard_blocked_only_when_the_hardware_flag_says_so() {
    let mut harness = harness().await;
    let mut objects = measured_objects();
    objects.extend([manager_object(false, false)]);
    enumerate(&mut harness, objects).await;

    let radio = harness
        .state
        .borrow_and_update()
        .wifi
        .expect("a wireless device");
    assert!(radio.blocked());
    assert!(!radio.enabled);
}
