use glimpse_dbus::Buses;
use tokio_util::sync::CancellationToken;
use zbus::zvariant::Value;

use super::*;

const DRIVE: &str = "/org/freedesktop/UDisks2/drives/Cruzer";
const DATA: &str = "/org/freedesktop/UDisks2/block_devices/sdb1";
const RESERVE: &str = "/org/freedesktop/UDisks2/block_devices/sdb2";
const FIXED_DRIVE: &str = "/org/freedesktop/UDisks2/drives/nvme1";
const FIXED_BLOCK: &str = "/org/freedesktop/UDisks2/block_devices/nvme0n1p3";

async fn removable() -> (
    Removable,
    Ctx<Removable>,
    tokio::sync::watch::Receiver<RemovableState>,
    tokio::sync::watch::Receiver<crate::ServiceState>,
) {
    let cancel = CancellationToken::new();
    let (events, _inbox) = tokio::sync::mpsc::channel(8);
    let (state, state_rx) = tokio::sync::watch::channel(RemovableState::default());
    let (health, health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
    let ctx = Ctx::<Removable>::new(
        events,
        &cancel,
        state,
        health,
        Buses::unavailable("no bus in tests"),
    );
    let service = Removable::start(&ctx, Config::from(&glimpse_config::Config::default()), ())
        .await
        .expect("starts");
    (service, ctx, state_rx, health_rx)
}

fn properties(pairs: Vec<(&str, Value<'static>)>) -> Properties {
    pairs
        .into_iter()
        .map(|(key, value)| {
            (
                key.to_owned(),
                OwnedValue::try_from(value).expect("a plain value"),
            )
        })
        .collect()
}

fn owned_path(path: &'static str) -> Value<'static> {
    Value::ObjectPath(path.try_into().expect("a path"))
}

fn owned_mount_points(paths: &[&str]) -> Value<'static> {
    let entries: Vec<Vec<u8>> = paths
        .iter()
        .map(|path| {
            let mut raw = path.as_bytes().to_vec();
            raw.push(0);
            raw
        })
        .collect();
    Value::from(entries)
}

fn removable_drive_interfaces() -> Interfaces {
    HashMap::from([(
        udisks2::DRIVE1.to_owned(),
        properties(vec![
            ("Vendor", "SanDisk".into()),
            ("Model", "Cruzer".into()),
            ("Removable", true.into()),
            ("Ejectable", true.into()),
            ("CanPowerOff", true.into()),
            ("MediaAvailable", true.into()),
            ("Size", 8_000_000_000u64.into()),
        ]),
    )])
}

fn fixed_drive_interfaces() -> Interfaces {
    HashMap::from([(
        udisks2::DRIVE1.to_owned(),
        properties(vec![
            ("Vendor", "Samsung".into()),
            ("Model", "SSD 980 PRO".into()),
            ("Removable", false.into()),
            ("MediaRemovable", false.into()),
            ("ConnectionBus", "".into()),
        ]),
    )])
}

fn mounted_volume() -> Interfaces {
    HashMap::from([
        (
            udisks2::BLOCK1.to_owned(),
            properties(vec![
                ("Drive", owned_path(DRIVE)),
                ("IdLabel", "DATA".into()),
                ("IdType", "vfat".into()),
                ("Size", 4_000_000_000u64.into()),
            ]),
        ),
        (
            udisks2::FILESYSTEM1.to_owned(),
            properties(vec![(
                "MountPoints",
                owned_mount_points(&["/run/media/alex/DATA"]),
            )]),
        ),
    ])
}

fn unmounted_volume() -> Interfaces {
    HashMap::from([
        (
            udisks2::BLOCK1.to_owned(),
            properties(vec![
                ("Drive", owned_path(DRIVE)),
                ("IdLabel", "RESERVE".into()),
                ("IdType", "exfat".into()),
                ("Size", 4_000_000_000u64.into()),
            ]),
        ),
        (udisks2::FILESYSTEM1.to_owned(), properties(vec![])),
    ])
}

fn session() -> Objects {
    BTreeMap::from([
        (DRIVE.to_owned(), removable_drive_interfaces()),
        (DATA.to_owned(), mounted_volume()),
        (RESERVE.to_owned(), unmounted_volume()),
    ])
}

async fn enumerated(service: &mut Removable, ctx: &Ctx<Removable>, objects: Objects) {
    service
        .handle(ctx, Input::Event(Event::Enumerated(Box::new(objects))))
        .await;
}

#[tokio::test]
async fn a_removable_drive_with_two_filesystem_blocks_publishes_one_drive_with_two_volumes() {
    let (mut service, ctx, state, _health) = removable().await;

    enumerated(&mut service, &ctx, session()).await;

    let published = state.borrow();
    assert_eq!(published.drives.len(), 1);
    assert_eq!(published.drives[0].id, DriveId(DRIVE.to_owned()));
    assert_eq!(published.drives[0].volumes.len(), 2);
}

#[tokio::test]
async fn a_hint_system_block_does_not_appear() {
    let (mut service, ctx, state, _health) = removable().await;
    let mut objects = session();
    let block = objects.get_mut(DATA).expect("the block");
    block
        .get_mut(udisks2::BLOCK1)
        .expect("the block interface")
        .insert(
            "HintSystem".to_owned(),
            OwnedValue::try_from(Value::from(true)).expect("a flag"),
        );

    enumerated(&mut service, &ctx, objects).await;

    let published = state.borrow();
    assert_eq!(
        published.drives[0].volumes.len(),
        1,
        "a HintSystem block must not appear"
    );
}

#[tokio::test]
async fn a_hint_ignore_block_does_not_appear() {
    let (mut service, ctx, state, _health) = removable().await;
    let mut objects = session();
    let block = objects.get_mut(DATA).expect("the block");
    block
        .get_mut(udisks2::BLOCK1)
        .expect("the block interface")
        .insert(
            "HintIgnore".to_owned(),
            OwnedValue::try_from(Value::from(true)).expect("a flag"),
        );

    enumerated(&mut service, &ctx, objects).await;

    let published = state.borrow();
    assert_eq!(
        published.drives[0].volumes.len(),
        1,
        "a HintIgnore block must not appear"
    );
}

#[tokio::test]
async fn a_block_with_no_filesystem_interface_does_not_appear() {
    let (mut service, ctx, state, _health) = removable().await;
    let mut objects = session();
    objects.insert(
        "/org/freedesktop/UDisks2/block_devices/sdb3".to_owned(),
        HashMap::from([(
            udisks2::BLOCK1.to_owned(),
            properties(vec![
                ("Drive", owned_path(DRIVE)),
                ("IdLabel", "BARE".into()),
            ]),
        )]),
    );

    enumerated(&mut service, &ctx, objects).await;

    let published = state.borrow();
    assert_eq!(
        published.drives[0].volumes.len(),
        2,
        "a block with no Filesystem interface must not become a volume"
    );
}

#[tokio::test]
async fn a_block_whose_drive_is_not_removable_does_not_appear() {
    let (mut service, ctx, state, _health) = removable().await;
    let objects = BTreeMap::from([
        (FIXED_DRIVE.to_owned(), fixed_drive_interfaces()),
        (
            FIXED_BLOCK.to_owned(),
            HashMap::from([
                (
                    udisks2::BLOCK1.to_owned(),
                    properties(vec![
                        ("Drive", owned_path(FIXED_DRIVE)),
                        ("IdLabel", "".into()),
                        ("IdType", "exfat".into()),
                    ]),
                ),
                (udisks2::FILESYSTEM1.to_owned(), properties(vec![])),
            ]),
        ),
    ]);

    enumerated(&mut service, &ctx, objects).await;

    let published = state.borrow();
    assert!(
        published.drives.is_empty(),
        "a non-removable drive must not appear, nor its blocks"
    );
}

#[tokio::test]
async fn automount_hint_without_removable_drive_is_hidden() {
    let (mut service, ctx, state, _health) = removable().await;
    let objects = BTreeMap::from([
        (FIXED_DRIVE.to_owned(), fixed_drive_interfaces()),
        (
            FIXED_BLOCK.to_owned(),
            HashMap::from([
                (
                    udisks2::BLOCK1.to_owned(),
                    properties(vec![
                        ("Drive", owned_path(FIXED_DRIVE)),
                        ("HintAuto", true.into()),
                        ("IdType", "exfat".into()),
                    ]),
                ),
                (udisks2::FILESYSTEM1.to_owned(), properties(vec![])),
            ]),
        ),
    ]);

    enumerated(&mut service, &ctx, objects).await;

    let published = state.borrow();
    assert!(
        published.drives.is_empty(),
        "HintAuto alone must never be read as removability"
    );
}

#[tokio::test]
async fn an_unmounted_volume_has_no_mount_and_a_mounted_one_carries_its_path() {
    let (mut service, ctx, state, _health) = removable().await;

    enumerated(&mut service, &ctx, session()).await;

    let published = state.borrow();
    let mounted = published
        .volume(&VolumeId(DATA.to_owned()))
        .expect("the mounted volume");
    assert_eq!(
        mounted.mount.as_ref().map(|mount| mount.at.as_path()),
        Some(std::path::Path::new("/run/media/alex/DATA"))
    );

    let unmounted = published
        .volume(&VolumeId(RESERVE.to_owned()))
        .expect("the unmounted volume");
    assert_eq!(unmounted.mount, None);
}

#[tokio::test]
async fn eject_unmounts_only_this_drives_mounted_volumes_first() {
    let (mut service, ctx, _state, _health) = removable().await;
    enumerated(&mut service, &ctx, session()).await;

    assert_eq!(
        service.mounted_on(DRIVE),
        vec![DATA.to_owned()],
        "eject unmounts the mounted volume and leaves the unmounted one alone"
    );
    assert!(
        service.mounted_on(FIXED_DRIVE).is_empty(),
        "a volume belonging to another drive is never unmounted by this drive's eject"
    );

    service
        .blocks
        .get_mut(DATA)
        .expect("the volume")
        .filesystem
        .as_mut()
        .expect("a filesystem")
        .mount_points = Some(Vec::new());
    assert!(
        service.mounted_on(DRIVE).is_empty(),
        "a drive with nothing mounted ejects straight through, unmounting nothing"
    );
}

#[tokio::test]
async fn busy_is_cleared_by_the_command_completing_and_never_by_the_property() {
    let (mut service, ctx, state, _health) = removable().await;
    enumerated(&mut service, &ctx, session()).await;
    service.blocks.get_mut(DATA).expect("the volume").busy = Some(Busy::Unmounting);
    service.publish();

    service
        .handle(
            &ctx,
            Input::Event(Event::PropertiesChanged {
                path: DATA.to_owned(),
                interface: udisks2::FILESYSTEM1.to_owned(),
                changed: properties(vec![("MountPoints", owned_mount_points(&[]))]),
            }),
        )
        .await;
    assert_eq!(
        state
            .borrow()
            .volume(&VolumeId(DATA.to_owned()))
            .and_then(|volume| volume.busy),
        Some(Busy::Unmounting),
        "a backend that answers success without moving the property would strand the row"
    );

    service
        .handle(
            &ctx,
            Input::Event(Event::VolumeSettled {
                id: VolumeId(DATA.to_owned()),
                busy: Busy::Unmounting,
                failure: None,
            }),
        )
        .await;
    assert_eq!(
        state
            .borrow()
            .volume(&VolumeId(DATA.to_owned()))
            .and_then(|volume| volume.busy),
        None
    );
}

#[tokio::test]
async fn a_command_with_no_bus_is_refused_rather_than_queued() {
    let (mut service, ctx, _state, _health) = removable().await;
    enumerated(&mut service, &ctx, session()).await;

    let (reply, result) = oneshot::channel();
    service
        .handle(
            &ctx,
            Input::Command(Command::Mount {
                id: VolumeId(DATA.to_owned()),
                reply,
            }),
        )
        .await;

    assert!(matches!(
        result.await,
        Ok(Err(RemovableError::Service(CommandError::Unavailable(_))))
    ));
}

#[tokio::test]
async fn a_lost_bus_degrades_and_keeps_the_last_state() {
    let (mut service, ctx, state, health) = removable().await;
    enumerated(&mut service, &ctx, session()).await;

    service
        .handle(&ctx, Input::Event(Event::NameOwner(None)))
        .await;

    assert_eq!(state.borrow().drives.len(), 1);
    assert!(matches!(
        &*health.borrow(),
        crate::ServiceState::Degraded { .. }
    ));
}

#[tokio::test]
async fn regaining_the_name_restarts_the_object_sources_but_not_the_name_watch() {
    let (mut service, ctx, _state, _health) = removable().await;

    service
        .handle(
            &ctx,
            Input::Event(Event::NameOwner(Some(":1.7".to_owned()))),
        )
        .await;

    let declared = service.subscriptions();
    let keys: Vec<&Watch> = declared.iter().map(Sub::key).collect();
    assert_eq!(
        keys,
        vec![&Watch::NameOwner, &Watch::Objects(1), &Watch::Properties(1)]
    );
}

#[tokio::test]
async fn a_partial_change_keeps_what_it_does_not_carry() {
    let (mut service, ctx, state, _health) = removable().await;
    enumerated(&mut service, &ctx, session()).await;

    service
        .handle(
            &ctx,
            Input::Event(Event::PropertiesChanged {
                path: DATA.to_owned(),
                interface: udisks2::BLOCK1.to_owned(),
                changed: properties(vec![("Size", 5_000_000_000u64.into())]),
            }),
        )
        .await;

    let published = state.borrow();
    let volume = published
        .volume(&VolumeId(DATA.to_owned()))
        .expect("the volume");
    assert_eq!(
        volume.label, "DATA",
        "the label must survive an unrelated update"
    );
    assert_eq!(volume.size, 5_000_000_000);
}

#[tokio::test]
async fn no_capacity_subscription_exists_with_nothing_mounted_and_one_appears_with_a_mount() {
    let (mut service, ctx, _state, _health) = removable().await;

    let objects = BTreeMap::from([
        (DRIVE.to_owned(), removable_drive_interfaces()),
        (RESERVE.to_owned(), unmounted_volume()),
    ]);
    enumerated(&mut service, &ctx, objects).await;
    assert!(
        !service
            .subscriptions()
            .iter()
            .any(|sub| matches!(sub.key(), Watch::Capacity)),
        "nothing is mounted, so no timer should run"
    );

    enumerated(&mut service, &ctx, session()).await;
    assert!(
        service
            .subscriptions()
            .iter()
            .any(|sub| matches!(sub.key(), Watch::Capacity)),
        "a mounted volume must declare the capacity source"
    );
}

#[tokio::test]
async fn a_zero_capacity_interval_never_declares_the_subscription() {
    let (mut service, ctx, _state, _health) = removable().await;
    service.config.capacity_interval = None;

    enumerated(&mut service, &ctx, session()).await;

    assert!(
        !service
            .subscriptions()
            .iter()
            .any(|sub| matches!(sub.key(), Watch::Capacity)),
        "capacity_interval = 0 must turn the sampler off even with a mount present"
    );
}

#[tokio::test]
async fn capacity_sampling_runs_off_the_handler_and_reaches_the_mounted_volume() {
    let cancel = CancellationToken::new();
    let (events, mut inbox) = tokio::sync::mpsc::channel(8);
    let (state, state_rx) = tokio::sync::watch::channel(RemovableState::default());
    let (health, _health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
    let ctx = Ctx::<Removable>::new(
        events,
        &cancel,
        state,
        health,
        Buses::unavailable("no bus in tests"),
    );
    let mut service = Removable::start(&ctx, Config::from(&glimpse_config::Config::default()), ())
        .await
        .expect("starts");

    let objects = BTreeMap::from([
        (DRIVE.to_owned(), removable_drive_interfaces()),
        (
            DATA.to_owned(),
            HashMap::from([
                (
                    udisks2::BLOCK1.to_owned(),
                    properties(vec![
                        ("Drive", owned_path(DRIVE)),
                        ("IdLabel", "DATA".into()),
                    ]),
                ),
                (
                    udisks2::FILESYSTEM1.to_owned(),
                    properties(vec![(
                        "MountPoints",
                        owned_mount_points(&[std::env::temp_dir().to_str().expect("utf8 path")]),
                    )]),
                ),
            ]),
        ),
    ]);
    enumerated(&mut service, &ctx, objects).await;

    service
        .handle(&ctx, Input::Event(Event::CapacityTick))
        .await;
    let delivered = tokio::time::timeout(std::time::Duration::from_secs(5), inbox.recv())
        .await
        .expect("the sample arrives")
        .expect("the channel stays open");
    service.handle(&ctx, delivered).await;

    let published = state_rx.borrow();
    let volume = published
        .volume(&VolumeId(DATA.to_owned()))
        .expect("the volume");
    let capacity = volume
        .mount
        .as_ref()
        .expect("a mount")
        .capacity
        .expect("statvfs succeeds against a real temp directory");
    assert!(capacity.total > 0);
}

#[tokio::test]
async fn the_state_before_any_event_has_arrived_is_empty_and_the_service_is_not_degraded() {
    let (_service, _ctx, state, health) = removable().await;

    assert_eq!(*state.borrow(), RemovableState::default());
    assert!(!matches!(
        &*health.borrow(),
        crate::ServiceState::Degraded { .. }
    ));
}

#[test]
fn a_drive_with_no_vendor_or_model_falls_back_to_its_serial_then_a_generic_name() {
    assert_eq!(
        drive_name(&DriveProperties {
            serial: Some("SN123".to_owned()),
            ..Default::default()
        }),
        "SN123"
    );
    assert_eq!(drive_name(&DriveProperties::default()), "Removable drive");
    assert_eq!(
        drive_name(&DriveProperties {
            vendor: Some("SanDisk".to_owned()),
            model: Some("Cruzer".to_owned()),
            ..Default::default()
        }),
        "SanDisk Cruzer"
    );
}
