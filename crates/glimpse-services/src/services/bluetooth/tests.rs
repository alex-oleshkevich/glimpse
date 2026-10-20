use glimpse_dbus::Buses;

use tokio_util::sync::CancellationToken;

use zbus::zvariant::Value;

use super::*;

const ADAPTER: &str = "/org/bluez/hci0";

const HEADSET: &str = "/org/bluez/hci0/dev_F8_4E_17_BC_EE_D5";

const NEARBY: &str = "/org/bluez/hci0/dev_45_16_94_89_4F_38";

async fn bluetooth() -> (
    Bluetooth,
    Ctx<Bluetooth>,
    tokio::sync::watch::Receiver<BluetoothState>,
    tokio::sync::watch::Receiver<crate::ServiceState>,
) {
    started(Dependencies { agent: true }).await
}

async fn started(
    dependencies: Dependencies,
) -> (
    Bluetooth,
    Ctx<Bluetooth>,
    tokio::sync::watch::Receiver<BluetoothState>,
    tokio::sync::watch::Receiver<crate::ServiceState>,
) {
    let cancel = CancellationToken::new();
    let (events, _inbox) = tokio::sync::mpsc::channel(8);
    let (state, state_rx) = tokio::sync::watch::channel(BluetoothState::default());
    let (health, health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
    let ctx = Ctx::<Bluetooth>::new(
        events,
        &cancel,
        state,
        health,
        Buses::unavailable("no bus in tests"),
    );
    let service = Bluetooth::start(
        &ctx,
        Config::from(&glimpse_config::Config::default()),
        dependencies,
    )
    .await;
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

fn adapter() -> Interfaces {
    HashMap::from([(
        bluez::ADAPTER1.to_owned(),
        properties(vec![
            ("Alias", "glimpse".into()),
            ("Powered", true.into()),
            ("PowerState", "on".into()),
            ("Discoverable", false.into()),
            ("Discovering", false.into()),
        ]),
    )])
}

fn headset() -> Interfaces {
    HashMap::from([(
        bluez::DEVICE1.to_owned(),
        properties(vec![
            ("Address", "F8:4E:17:BC:EE:D5".into()),
            ("Alias", "WH-1000XM4".into()),
            ("Icon", "audio-headset".into()),
            ("Paired", true.into()),
            ("Bonded", true.into()),
            ("Trusted", true.into()),
            ("Connected", true.into()),
        ]),
    )])
}

fn nearby() -> Interfaces {
    HashMap::from([(
        bluez::DEVICE1.to_owned(),
        properties(vec![
            ("Address", "45:16:94:89:4F:38".into()),
            ("Alias", "[TV] Samsung AU7172".into()),
            ("RSSI", (-73i16).into()),
        ]),
    )])
}

fn beacon() -> Interfaces {
    HashMap::from([(
        bluez::DEVICE1.to_owned(),
        properties(vec![
            ("Address", "6B:2C:11:04:9A:71".into()),
            ("Alias", "6B-2C-11-04-9A-71".into()),
            ("RSSI", (-91i16).into()),
        ]),
    )])
}

fn session() -> Objects {
    BTreeMap::from([
        (ADAPTER.to_owned(), adapter()),
        (HEADSET.to_owned(), headset()),
        (NEARBY.to_owned(), nearby()),
    ])
}

async fn enumerated(service: &mut Bluetooth, ctx: &Ctx<Bluetooth>, objects: Objects) {
    service
        .handle(ctx, Input::Event(Event::Enumerated(Box::new(objects))))
        .await;
}

async fn command(
    service: &mut Bluetooth,
    ctx: &Ctx<Bluetooth>,
    build: impl FnOnce(Reply) -> Command,
) -> Result<(), BluetoothError> {
    let (reply, outcome) = oneshot::channel();
    service.handle(ctx, Input::Command(build(reply))).await;
    outcome.await.expect("the command was answered")
}

fn device(name: &str, connected: bool) -> Device {
    Device {
        id: DeviceId::new(format!("/org/bluez/hci0/dev_{name}")),
        address: "00:00:00:00:00:00".to_owned(),
        name: name.to_owned(),
        icon: DeviceIcon::Headset,
        paired: true,
        bonded: true,
        trusted: true,
        blocked: false,
        connected,
        battery: None,
        codec: None,
        rssi: None,
        profiles: Vec::new(),
        busy: None,
        failure: None,
    }
}

fn state(power: Power, scanning: bool, devices: Vec<Device>) -> BluetoothState {
    BluetoothState {
        adapter: Some(Adapter {
            alias: "glimpse".to_owned(),
            power,
            discoverable: false,
        }),
        devices,
        scan: scanning.then_some(Hold::Held),
        pairing: None,
        confirm: None,
    }
}

mod commands;
mod confirmation;
mod connect;
mod devices;
mod failures;
mod icon;
mod mirror;
mod pairing;
mod prompts;
mod properties;
mod review;
mod scanning;
