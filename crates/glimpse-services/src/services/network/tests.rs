use super::*;
use zbus::zvariant::Value;

fn owned<'a, T: Into<Value<'a>>>(value: T) -> OwnedValue {
    OwnedValue::try_from(value.into()).expect("a representable value")
}

fn properties(pairs: Vec<(&str, OwnedValue)>) -> Properties {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
}

fn device(kind: u32, managed: bool) -> nm::DeviceProperties {
    nm::decode_device(&properties(vec![
        ("DeviceType", owned(kind)),
        ("Managed", owned(managed)),
        ("State", owned(100u32)),
    ]))
}

fn access(ssid: &str, strength: u8, flags: u32, active: bool) -> Access {
    Access {
        id: NetworkId::new(format!("/ap/{ssid}/{strength}")),
        ssid: (!ssid.is_empty()).then(|| ssid.to_owned()),
        bssid: Some(format!("00:00:00:00:00:{strength:02x}")),
        strength,
        band: nm::Band::TwoPointFour,
        security: nm::Security::read(flags, 0, 392),
        active,
        saved: None,
        busy: None,
        failure: None,
    }
}

#[test]
fn only_a_device_a_user_can_act_on_reaches_the_model() {
    assert!(wanted(&device(2, true)), "wlp99s0");
    assert!(
        wanted(&device(1, true)),
        "an ethernet device, were there one"
    );
    assert!(wanted(&device(8, true)), "a modem, were there one");

    assert!(!wanted(&device(13, true)), "docker0 and the two br-*");
    assert!(!wanted(&device(20, false)), "the two unmanaged veths");
    assert!(!wanted(&device(32, true)), "lo");
    assert!(!wanted(&device(5, true)), "the bluetooth NAP");
    assert!(!wanted(&device(30, true)), "p2p-dev-wlp99s0");
}

#[test]
fn an_unmanaged_device_is_out_even_when_its_type_is_one_we_want() {
    assert!(!wanted(&device(2, false)));
}

#[test]
fn the_connected_beacon_survives_a_stronger_sibling_on_the_same_ssid() {
    let deduped = strongest(
        vec![
            access("Skylink", 70, 3, true),
            access("Skylink", 94, 3, false),
        ]
        .into_iter(),
    );

    assert_eq!(deduped.len(), 1);
    assert!(
        deduped[0].active,
        "the access point in use is often not the strongest, and dropping it for its sibling \
             leaves the bar reporting no connection while NetworkManager still has one"
    );
    assert_eq!(deduped[0].strength, 70);

    let other_way = strongest(
        vec![
            access("Skylink", 94, 3, false),
            access("Skylink", 70, 3, true),
        ]
        .into_iter(),
    );
    assert!(
        other_way[0].active,
        "and it survives whichever order the two arrive in"
    );
}

#[test]
fn access_points_deduplicate_by_ssid_keeping_the_strongest_whole_one() {
    let deduped = strongest(
        vec![
            access("RubinowyKlon1", 60, 3, false),
            access("RubinowyKlon1", 40, 1, false),
            access("UPC6516723", 60, 3, false),
            access("UPC6516723", 45, 3, false),
        ]
        .into_iter(),
    );

    assert_eq!(deduped.len(), 2, "four beacons, two networks");
    let rubinowy = deduped
        .iter()
        .find(|one| one.ssid.as_deref() == Some("RubinowyKlon1"))
        .expect("the network");
    assert_eq!(rubinowy.strength, 60);
    assert_eq!(
        rubinowy.security,
        nm::Security::read(3, 0, 392),
        "the whole stronger AP wins; merging flags would invent a security level"
    );
}

#[test]
fn the_connected_network_is_placed_first_even_when_it_is_not_the_strongest() {
    let ordered = strongest(
        vec![
            access("PLAY internet 2.4GHz_2938", 94, 3, false),
            access("Skylink", 70, 3, true),
        ]
        .into_iter(),
    );

    assert_eq!(
        ordered.first().and_then(|one| one.ssid.as_deref()),
        Some("Skylink")
    );
    assert!(ordered[0].active);
}

#[test]
fn an_unnamed_beacon_survives_dedup_as_its_own_row() {
    let deduped = strongest(
        vec![
            access("", 42, 1, false),
            access("", 30, 1, false),
            access("Skylink", 70, 3, false),
        ]
        .into_iter(),
    );

    let unnamed = deduped.iter().filter(|one| one.ssid.is_none()).count();
    assert_eq!(unnamed, 2, "two unnamed beacons are two networks, not one");
}

#[test]
fn the_relevance_allowlist_wakes_on_what_the_model_reads_and_not_on_churn() {
    assert!(relevant(&properties(vec![("Strength", owned(70u8))]), &[]));
    assert!(relevant(&properties(vec![("State", owned(100u32))]), &[]));
    assert!(relevant(
        &properties(vec![("Connectivity", owned(4u32))]),
        &[]
    ));

    assert!(
        !relevant(&properties(vec![("Bitrate", owned(960700u32))]), &[]),
        "Bitrate churned once in thirty idle seconds and feeds nothing"
    );
    assert!(
        !relevant(
            &properties(vec![("RxBytes", owned(1u64)), ("TxBytes", owned(2u64))]),
            &[]
        ),
        "Device.Statistics is writable by any client on the bus"
    );
}

#[test]
fn an_invalidated_property_the_model_reads_still_wakes_it() {
    assert!(relevant(&Properties::new(), &["Ssid".to_owned()]));
    assert!(!relevant(&Properties::new(), &["Bitrate".to_owned()]));
}

#[test]
fn a_radio_reports_blocked_only_when_the_hardware_says_so() {
    assert!(
        !Radio {
            enabled: false,
            hardware_enabled: true
        }
        .blocked(),
        "soft off is not blocked; the switch still works"
    );
    assert!(
        Radio {
            enabled: false,
            hardware_enabled: false
        }
        .blocked()
    );
}

#[test]
fn a_portal_does_not_reach_the_internet_but_an_unknown_connectivity_does() {
    let portal = NetworkState {
        connectivity: nm::Connectivity::Portal,
        ..NetworkState::default()
    };
    assert!(!portal.reaches_the_internet());

    let unchecked = NetworkState {
        connectivity: nm::Connectivity::Unknown,
        ..NetworkState::default()
    };
    assert!(
        unchecked.reaches_the_internet(),
        "a disabled connectivity check must not render as no internet"
    );
}

pub(super) mod service {
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
        let service = Network::start(&ctx, config, ())
            .await
            .expect("the service starts");

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
}

mod secrets {
    use super::service::*;
    use super::*;
    use zeroize::Zeroizing;

    #[tokio::test]
    async fn a_second_secret_request_is_refused_rather_than_queued() {
        let mut harness = harness().await;
        let (first, _first_reply) = tokio::sync::oneshot::channel();
        let (second, second_reply) = tokio::sync::oneshot::channel();

        let ask = |name: &str| Request {
            name: name.to_owned(),
            path: "/org/freedesktop/NetworkManager/Settings/1".to_owned(),
            setting: "802-11-wireless-security".to_owned(),
            retry: false,
        };

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Secrets(ask("Skylink"), first)),
            )
            .await;
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Secrets(ask("Skylink 2G"), second)),
            )
            .await;

        let refused = tokio::time::timeout(std::time::Duration::from_secs(2), second_reply)
            .await
            .expect("the second request must be answered, not held open");
        assert!(
            matches!(refused, Ok(Answer::Refused)),
            "two password dialogs at once cannot be told apart by whoever answers them"
        );
        assert_eq!(
            harness
                .service
                .secret
                .as_ref()
                .map(|(request, _)| request.name.clone()),
            Some("Skylink".to_owned()),
            "the first prompt is the one that stays open"
        );
    }

    #[tokio::test]
    async fn answering_a_prompt_resolves_the_agent_and_clears_the_state() {
        let mut harness = harness().await;
        let (sender, reply) = tokio::sync::oneshot::channel();

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Secrets(
                    Request {
                        name: "Skylink".to_owned(),
                        path: "/org/freedesktop/NetworkManager/Settings/1".to_owned(),
                        setting: "802-11-wireless-security".to_owned(),
                        retry: true,
                    },
                    sender,
                )),
            )
            .await;
        assert!(harness.service.secret.is_some());

        let (ack, _acked) = tokio::sync::oneshot::channel();
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Command(Command::AnswerSecret {
                    answer: Answer::Secret(Zeroizing::new("hunter2hunter2".to_owned())),
                    reply: ack,
                }),
            )
            .await;

        let answered = tokio::time::timeout(std::time::Duration::from_secs(2), reply)
            .await
            .expect("the agent must be resolved, not left waiting");
        assert!(matches!(answered, Ok(Answer::Secret(_))));
        assert!(harness.service.secret.is_none());
    }

    #[tokio::test]
    async fn a_cancellation_naming_another_request_leaves_the_open_prompt_alone() {
        let mut harness = harness().await;
        let (sender, reply) = tokio::sync::oneshot::channel();

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Secrets(
                    Request {
                        name: "Skylink".to_owned(),
                        path: "/org/freedesktop/NetworkManager/Settings/1".to_owned(),
                        setting: "802-11-wireless-security".to_owned(),
                        retry: false,
                    },
                    sender,
                )),
            )
            .await;

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::SecretsCancelled {
                    path: "/org/freedesktop/NetworkManager/Settings/9".to_owned(),
                    setting: "802-11-wireless-security".to_owned(),
                }),
            )
            .await;
        assert!(
            harness.service.secret.is_some(),
            "a late cancellation for a request already answered would otherwise refuse the one \
             the user is looking at"
        );

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::SecretsCancelled {
                    path: "/org/freedesktop/NetworkManager/Settings/1".to_owned(),
                    setting: "802-11-wireless-security".to_owned(),
                }),
            )
            .await;
        assert!(harness.service.secret.is_none());
        assert!(matches!(reply.await, Ok(Answer::Refused)));
    }

    #[tokio::test]
    async fn networkmanager_leaving_the_bus_refuses_an_open_prompt() {
        let mut harness = harness().await;
        let (sender, reply) = tokio::sync::oneshot::channel();

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Secrets(
                    Request {
                        name: "Skylink".to_owned(),
                        path: "/org/freedesktop/NetworkManager/Settings/1".to_owned(),
                        setting: "802-11-wireless-security".to_owned(),
                        retry: false,
                    },
                    sender,
                )),
            )
            .await;
        harness
            .service
            .handle(&harness.ctx, Input::Event(Event::NameOwner(None)))
            .await;

        let refused = tokio::time::timeout(std::time::Duration::from_secs(2), reply)
            .await
            .expect("a prompt left open against a dead NetworkManager would hang forever");
        assert!(matches!(refused, Ok(Answer::Refused)));
    }
}

mod scanning {
    use super::service::*;
    use super::*;

    async fn start(harness: &mut Harness) {
        let (reply, _result) = tokio::sync::oneshot::channel();
        harness
            .service
            .handle(&harness.ctx, Input::Command(Command::StartScan { reply }))
            .await;
    }

    #[tokio::test]
    async fn a_scan_takes_its_deadline_from_the_configured_timeout() {
        let mut harness = harness().await;
        enumerate(&mut harness, measured_objects()).await;

        start(&mut harness).await;
        assert!(
            harness.service.deadline.is_some(),
            "the default [network] scan-timeout is what stops a scan nobody closed"
        );

        assert!(
            harness
                .service
                .subscriptions()
                .iter()
                .any(|sub| matches!(sub.key(), Watch::ScanDeadline(_))),
            "the deadline is declared as a subscription, not started by hand"
        );

        let forever = glimpse_config::Config {
            network: glimpse_config::NetworkSettings {
                scan_timeout: 0,
                ..glimpse_config::NetworkSettings::default()
            },
            ..glimpse_config::Config::default()
        };
        harness
            .service
            .handle(&harness.ctx, Input::Config(Config::from(&forever)))
            .await;
        start(&mut harness).await;
        assert!(
            harness.service.deadline.is_none(),
            "zero disables the timeout and leaves the scan running until the popover closes"
        );
    }

    #[tokio::test]
    async fn a_radio_that_is_off_refuses_a_scan_rather_than_asking_networkmanager() {
        let mut harness = harness().await;
        enumerate(&mut harness, measured_objects()).await;
        harness.service.manager.wireless_enabled = Some(false);

        let (reply, result) = tokio::sync::oneshot::channel();
        harness
            .service
            .handle(&harness.ctx, Input::Command(Command::StartScan { reply }))
            .await;

        assert!(
            matches!(
                result.await.expect("the sender was not dropped"),
                Err(NetworkError::Failed(Failure::NoDevice))
            ),
            "a popover opening over a switched-off radio must not ask the bus to scan"
        );
        assert!(harness.service.scan.is_none());
    }

    #[tokio::test]
    async fn a_stale_deadline_cannot_stop_the_scan_that_replaced_it() {
        let mut harness = harness().await;
        enumerate(&mut harness, measured_objects()).await;

        start(&mut harness).await;
        let first = harness.service.scan.expect("a scan");
        start(&mut harness).await;

        harness
            .service
            .handle(&harness.ctx, Input::Event(Event::ScanExpired(first)))
            .await;

        assert!(
            harness.service.scan.is_some(),
            "the generation is what makes an expired older scan harmless"
        );
    }

    #[tokio::test]
    async fn its_own_deadline_stops_it() {
        let mut harness = harness().await;
        enumerate(&mut harness, measured_objects()).await;
        start(&mut harness).await;
        let scan = harness.service.scan.expect("a scan");

        harness
            .service
            .handle(&harness.ctx, Input::Event(Event::ScanExpired(scan)))
            .await;

        assert!(harness.service.scan.is_none());
        assert!(harness.service.deadline.is_none());
        assert!(!harness.state.borrow_and_update().scanning);
    }

    #[tokio::test]
    async fn stopping_is_bookkeeping_and_answers_even_with_no_bus() {
        let mut harness = harness().await;
        enumerate(&mut harness, measured_objects()).await;
        start(&mut harness).await;

        let (reply, result) = tokio::sync::oneshot::channel();
        harness
            .service
            .handle(&harness.ctx, Input::Command(Command::StopScan { reply }))
            .await;

        let outcome = tokio::time::timeout(std::time::Duration::from_secs(1), result)
            .await
            .expect("it answers rather than waiting for a bus")
            .expect("the sender lived");
        assert!(
            outcome.is_ok(),
            "NetworkManager has no explicit stop to call"
        );
        assert!(harness.service.scan.is_none());
    }

    #[tokio::test]
    async fn the_radio_going_away_clears_a_running_scan() {
        let mut harness = harness().await;
        enumerate(&mut harness, measured_objects()).await;
        start(&mut harness).await;

        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::PropertiesChanged {
                    path: nm::MANAGER.to_owned(),
                    interface: nm::MANAGER1.to_owned(),
                    changed: properties(vec![("WirelessEnabled", owned(false))]),
                    invalidated: Vec::new(),
                }),
            )
            .await;

        assert!(
            harness.service.scan.is_none(),
            "a scan cannot outlive the radio it runs on"
        );
    }

    #[tokio::test]
    async fn networkmanager_disappearing_clears_a_running_scan() {
        let mut harness = harness().await;
        enumerate(&mut harness, measured_objects()).await;
        start(&mut harness).await;

        harness
            .service
            .handle(&harness.ctx, Input::Event(Event::NameOwner(None)))
            .await;

        assert!(harness.service.scan.is_none());
    }
}

mod profiles {
    use super::service::*;
    use super::*;

    fn profile(ssid: &str, timestamp: u64) -> nm::Profile {
        nm::Profile {
            id: Some(ssid.to_owned()),
            uuid: Some(format!("uuid-{ssid}-{timestamp}")),
            kind: Some("802-11-wireless".to_owned()),
            ssid: Some(ssid.to_owned()),
            timestamp,
            autoconnect: true,
            ..nm::Profile::default()
        }
    }

    #[tokio::test]
    async fn the_most_recently_used_profile_wins_for_a_shared_ssid() {
        let mut harness = harness().await;
        enumerate(&mut harness, measured_objects()).await;

        harness.service.profiles.insert(
            "/org/freedesktop/NetworkManager/Settings/1".to_owned(),
            profile("Skylink", 100),
        );
        harness.service.profiles.insert(
            "/org/freedesktop/NetworkManager/Settings/9".to_owned(),
            profile("Skylink", 900),
        );

        assert_eq!(
            harness
                .service
                .profile_matching(Some("Skylink"), None)
                .map(|id| id.as_str().to_owned()),
            Some("/org/freedesktop/NetworkManager/Settings/9".to_owned()),
            "the newer timestamp is the one the user last actually used"
        );
    }

    fn beacon(ssid: &str, rsn: u32, bssid: &str) -> nm::AccessPointProperties {
        nm::AccessPointProperties {
            ssid: Some(ssid.to_owned()),
            raw_ssid: ssid.as_bytes().to_vec(),
            bssid: Some(bssid.to_owned()),
            flags: 1,
            rsn_flags: rsn,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn a_profile_that_cannot_join_the_beacon_is_never_offered_for_it() {
        let mut harness = harness().await;
        enumerate(&mut harness, measured_objects()).await;

        let mut open = profile("CorpNet", 900);
        open.key_mgmt = None;
        let mut secured = profile("CorpNet", 100);
        secured.key_mgmt = Some("wpa-psk".to_owned());
        harness.service.profiles.insert("/s/open".to_owned(), open);
        harness
            .service
            .profiles
            .insert("/s/secured".to_owned(), secured);

        let point = beacon("CorpNet", 392, "00:11:22:33:44:55");
        assert_eq!(
            harness
                .service
                .profile_matching(Some("CorpNet"), Some(&point))
                .map(|id| id.as_str().to_owned()),
            Some("/s/secured".to_owned()),
            "the newer open profile cannot join a WPA2 beacon, so choosing it by timestamp alone \
             fails the join without ever asking for a password"
        );
    }

    #[tokio::test]
    async fn a_profile_that_has_seen_this_beacon_outranks_a_newer_one_that_has_not() {
        let mut harness = harness().await;
        enumerate(&mut harness, measured_objects()).await;

        let mut seen = profile("Skylink", 100);
        seen.key_mgmt = Some("wpa-psk".to_owned());
        seen.seen_bssids = vec!["00:11:22:33:44:55".to_owned()];
        let mut newer = profile("Skylink", 900);
        newer.key_mgmt = Some("wpa-psk".to_owned());
        harness.service.profiles.insert("/s/seen".to_owned(), seen);
        harness
            .service
            .profiles
            .insert("/s/newer".to_owned(), newer);

        let point = beacon("Skylink", 392, "00:11:22:33:44:55");
        assert_eq!(
            harness
                .service
                .profile_matching(Some("Skylink"), Some(&point))
                .map(|id| id.as_str().to_owned()),
            Some("/s/seen".to_owned())
        );
    }

    #[tokio::test]
    async fn a_tie_on_timestamp_is_broken_by_path_so_the_answer_never_moves() {
        let mut harness = harness().await;
        enumerate(&mut harness, measured_objects()).await;

        for path in ["/s/2", "/s/1", "/s/3"] {
            harness
                .service
                .profiles
                .insert(path.to_owned(), profile("Skylink", 0));
        }

        let first = harness.service.profile_matching(Some("Skylink"), None);
        for _ in 0..5 {
            assert_eq!(
                harness.service.profile_matching(Some("Skylink"), None),
                first,
                "an unchanged set must answer the same way every time"
            );
        }
        assert_eq!(
            first.map(|id| id.as_str().to_owned()),
            Some("/s/1".to_owned()),
            "the lowest path breaks the tie"
        );
    }

    async fn autoconnect(harness: &mut Harness, id: &str) -> Result<(), NetworkError> {
        let (reply, result) = tokio::sync::oneshot::channel();
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Command(Command::SetAutoconnect {
                    id: NetworkId::new(id),
                    autoconnect: false,
                    reply,
                }),
            )
            .await;
        tokio::time::timeout(std::time::Duration::from_secs(1), result)
            .await
            .expect("the reply arrives rather than hanging")
            .expect("the sender was not dropped")
    }

    #[tokio::test]
    async fn a_profile_setting_reached_from_an_access_point_row_resolves_to_the_profile() {
        let mut harness = harness().await;
        enumerate(&mut harness, measured_objects()).await;
        harness
            .service
            .profiles
            .insert("/s/1".to_owned(), profile("Skylink", 10));
        harness.service.access_points.insert(
            "/org/freedesktop/NetworkManager/AccessPoint/1".to_owned(),
            nm::AccessPointProperties {
                ssid: Some("Skylink".to_owned()),
                ..nm::AccessPointProperties::default()
            },
        );
        harness.service.access_points.insert(
            "/org/freedesktop/NetworkManager/AccessPoint/2".to_owned(),
            nm::AccessPointProperties {
                ssid: Some("PLAY internet".to_owned()),
                ..nm::AccessPointProperties::default()
            },
        );

        assert_eq!(
            harness.service.profile_of(&NetworkId::new(
                "/org/freedesktop/NetworkManager/AccessPoint/1"
            )),
            Some("/s/1".to_owned()),
            "the access point resolves to the profile saved for its ssid"
        );
        assert!(
            matches!(
                autoconnect(
                    &mut harness,
                    "/org/freedesktop/NetworkManager/AccessPoint/1"
                )
                .await,
                Err(NetworkError::Unavailable(_))
            ),
            "the detail drawer of a networks row carries the access point path, and \
             Settings.Connection does not exist on one - the saved profile behind it is \
             what the call has to name"
        );
        assert!(
            matches!(
                autoconnect(&mut harness, "/s/1").await,
                Err(NetworkError::Unavailable(_))
            ),
            "a known-networks row already carries the profile path"
        );
        assert!(
            matches!(
                autoconnect(
                    &mut harness,
                    "/org/freedesktop/NetworkManager/AccessPoint/2"
                )
                .await,
                Err(NetworkError::Failed(Failure::NotFound))
            ),
            "an access point nothing is saved for has no profile to write"
        );
    }

    #[tokio::test]
    async fn a_network_with_no_saved_profile_has_none() {
        let mut harness = harness().await;
        enumerate(&mut harness, measured_objects()).await;
        harness
            .service
            .profiles
            .insert("/s/1".to_owned(), profile("Skylink", 10));

        assert_eq!(
            harness
                .service
                .profile_matching(Some("PLAY internet"), None),
            None
        );
        assert_eq!(
            harness.service.profile_matching(None, None),
            None,
            "an unnamed network cannot match a profile by name"
        );
    }
}
