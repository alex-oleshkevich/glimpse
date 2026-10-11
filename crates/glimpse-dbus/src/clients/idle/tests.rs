use super::*;

struct OldIdle1;

#[zbus::interface(name = "me.aresa.Glimpse.Idle1")]
impl OldIdle1 {
    #[zbus(property)]
    fn inhibitors(&self) -> Vec<IdleInhibitorRecord> {
        Vec::new()
    }
}

#[tokio::test]
async fn a_missing_expiry_property_decodes_as_an_empty_map_without_an_error() {
    let bus = crate::testing::PrivateBus::start();
    let server = bus.connection().await;
    server
        .object_server()
        .at(GLIMPSE_IDLE_OBJECT_PATH, OldIdle1)
        .await
        .unwrap();
    crate::own_name(&server, GLIMPSE_IDLE_BUS_NAME)
        .await
        .unwrap();
    let client = bus.connection().await;
    let proxy = Idle1Proxy::new(&client).await.unwrap();
    assert!(
        cached_or_live_hold_expiries(&proxy)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn unknown_object_is_preserved_as_a_typed_call_error() {
    let request = std::future::ready(Err::<(), _>(
        zbus::fdo::Error::UnknownObject("gone".to_owned()).into(),
    ));
    assert!(matches!(
        call(request).await,
        Err(IdleProviderError::UnknownObject)
    ));
}

#[test]
fn wire_signatures_match_the_versioned_contract() {
    assert_eq!(SourceKind::SIGNATURE, "u");
    assert_eq!(InhibitionTargets::SIGNATURE, "(bbbbbbb)");
    assert_eq!(IdleInhibitorSource::SIGNATURE, "(uussuuu)");
    assert_eq!(BTreeMap::<u64, u64>::SIGNATURE, "a{tt}");
    assert_eq!(
        IdleInhibitorRecord::SIGNATURE,
        "(tssss(uussuuu)(bbbbbbb)bt)"
    );
    assert_eq!(
        InhibitorsHealth::SIGNATURE,
        "((us)(us)(us)(us))",
        "the wayland slot was appended; the OwnedValue derive pops one field per declared \
             field in order, so an older client drops the extra and only a removal would panic it"
    );
}

#[test]
fn from_login1_what_parses_known_tokens_and_ignores_unknown_ones() {
    let targets = InhibitionTargets::from_login1_what("idle:sleep:handle-lid-switch:bogus");
    assert_eq!(
        targets,
        InhibitionTargets {
            idle: true,
            suspend: true,
            lid_switch: true,
            ..InhibitionTargets::NONE
        }
    );
}

#[test]
fn from_login1_what_maps_each_token_to_exactly_its_own_field() {
    let cases = [
        (
            "idle",
            InhibitionTargets {
                idle: true,
                ..InhibitionTargets::NONE
            },
        ),
        (
            "sleep",
            InhibitionTargets {
                suspend: true,
                ..InhibitionTargets::NONE
            },
        ),
        (
            "shutdown",
            InhibitionTargets {
                shutdown: true,
                ..InhibitionTargets::NONE
            },
        ),
        (
            "handle-lid-switch",
            InhibitionTargets {
                lid_switch: true,
                ..InhibitionTargets::NONE
            },
        ),
        (
            "handle-power-key",
            InhibitionTargets {
                power_key: true,
                ..InhibitionTargets::NONE
            },
        ),
        (
            "handle-suspend-key",
            InhibitionTargets {
                suspend_key: true,
                ..InhibitionTargets::NONE
            },
        ),
        (
            "handle-hibernate-key",
            InhibitionTargets {
                hibernate_key: true,
                ..InhibitionTargets::NONE
            },
        ),
    ];
    for (token, expected) in cases {
        assert_eq!(
            InhibitionTargets::from_login1_what(token),
            expected,
            "token {token}"
        );
    }
}

#[test]
fn from_portal_flags_decodes_each_bit_independently() {
    assert_eq!(
        InhibitionTargets::from_portal_flags(0x8),
        InhibitionTargets {
            idle: true,
            ..InhibitionTargets::NONE
        }
    );
    assert_eq!(
        InhibitionTargets::from_portal_flags(0x4),
        InhibitionTargets {
            suspend: true,
            ..InhibitionTargets::NONE
        }
    );
    assert_eq!(
        InhibitionTargets::from_portal_flags(0x1),
        InhibitionTargets {
            shutdown: true,
            ..InhibitionTargets::NONE
        }
    );
}

#[test]
fn from_portal_flags_ignores_user_switch_in_any_combination() {
    assert_eq!(
        InhibitionTargets::from_portal_flags(12),
        InhibitionTargets {
            idle: true,
            suspend: true,
            ..InhibitionTargets::NONE
        },
        "flags=12 (Idle|Suspend)"
    );
    assert_eq!(
        InhibitionTargets::from_portal_flags(5),
        InhibitionTargets {
            shutdown: true,
            suspend: true,
            ..InhibitionTargets::NONE
        },
        "flags=5 (Logout|Suspend)"
    );
    assert_eq!(
        InhibitionTargets::from_portal_flags(2),
        InhibitionTargets::NONE,
        "flags=2 (UserSwitch alone) sets no target bit"
    );
    assert_eq!(
        InhibitionTargets::from_portal_flags(15),
        InhibitionTargets {
            idle: true,
            suspend: true,
            shutdown: true,
            ..InhibitionTargets::NONE
        },
        "flags=15 (all four) still drops UserSwitch's contribution"
    );
}

#[test]
fn manual_hold_sets_only_idle_and_suspend() {
    assert_eq!(
        InhibitionTargets::manual_hold(),
        InhibitionTargets {
            idle: true,
            suspend: true,
            ..InhibitionTargets::NONE
        }
    );
}

#[test]
fn source_constructors_populate_only_their_own_kinds_fields() {
    let screen_saver = IdleInhibitorSource::screen_saver(7);
    assert_eq!(screen_saver.kind, SourceKind::ScreenSaver);
    assert_eq!(screen_saver.cookie, 7);
    assert_eq!(screen_saver.app_id, "");
    assert_eq!(screen_saver.request_handle, "");
    assert_eq!(screen_saver.pid, 0);
    assert_eq!(screen_saver.uid, 0);

    let portal = IdleInhibitorSource::portal("/handle/1", "org.example.App");
    assert_eq!(portal.kind, SourceKind::Portal);
    assert_eq!(portal.request_handle, "/handle/1");
    assert_eq!(portal.app_id, "org.example.App");
    assert_eq!(portal.cookie, 0);
    assert_eq!(portal.pid, 0);
    assert_eq!(portal.uid, 0);

    let login1 = IdleInhibitorSource::login1(123, 1000, Login1Mode::Delay);
    assert_eq!(login1.kind, SourceKind::Login1);
    assert_eq!(login1.pid, 123);
    assert_eq!(login1.uid, 1000);
    assert_eq!(login1.mode, Login1Mode::Delay);
    assert_eq!(login1.cookie, 0);
    assert_eq!(login1.app_id, "");
    assert_eq!(login1.request_handle, "");
}

fn every_field_populated_record() -> IdleInhibitorRecord {
    IdleInhibitorRecord {
        id: 42,
        who: "Zoom".to_owned(),
        why: "screen sharing".to_owned(),
        bus_name: ":1.123".to_owned(),
        process_name: "zoom".to_owned(),
        source: IdleInhibitorSource {
            kind: SourceKind::Login1,
            cookie: 11,
            app_id: "org.example.App".to_owned(),
            request_handle: "/org/freedesktop/portal/desktop/request/1".to_owned(),
            pid: 555,
            uid: 1000,
            mode: Login1Mode::BlockWeak,
        },
        targets: InhibitionTargets {
            idle: true,
            suspend: false,
            shutdown: true,
            lid_switch: false,
            power_key: true,
            suspend_key: false,
            hibernate_key: true,
        },
        can_release: true,
        added_at_unix: 1_789_382_400,
    }
}

#[test]
fn a_record_round_trips_positionally_over_the_wire() {
    use zbus::zvariant::Endian;
    use zbus::zvariant::serialized::Context;

    type SourceWire = (u32, u32, String, String, u32, u32, u32);
    type TargetsWire = (bool, bool, bool, bool, bool, bool, bool);
    type RecordWire = (
        u64,
        String,
        String,
        String,
        String,
        SourceWire,
        TargetsWire,
        bool,
        u64,
    );

    let record = every_field_populated_record();

    let ctxt = Context::new_dbus(Endian::Little, 0);
    let encoded = zbus::zvariant::to_bytes(ctxt, &record).unwrap();
    let (wire, _): (RecordWire, _) = encoded.deserialize().unwrap();

    assert_eq!(
        wire,
        (
            record.id,
            record.who.clone(),
            record.why.clone(),
            record.bus_name.clone(),
            record.process_name.clone(),
            (
                record.source.kind as u32,
                record.source.cookie,
                record.source.app_id.clone(),
                record.source.request_handle.clone(),
                record.source.pid,
                record.source.uid,
                record.source.mode as u32,
            ),
            (
                record.targets.idle,
                record.targets.suspend,
                record.targets.shutdown,
                record.targets.lid_switch,
                record.targets.power_key,
                record.targets.suspend_key,
                record.targets.hibernate_key,
            ),
            record.can_release,
            record.added_at_unix,
        ),
        "a positional decode must land each value in the slot its field name implies, \
             which a JSON round trip (matched by key, not position) cannot verify"
    );
}
