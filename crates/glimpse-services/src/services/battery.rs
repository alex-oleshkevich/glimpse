use std::collections::BTreeMap;
use std::pin::Pin;

use futures_util::{Stream, StreamExt, stream};
use glimpse_dbus::power_profiles::{self, PowerProfilesDaemonProxy};
use glimpse_dbus::upower::{
    self, ChargeState, ChargeThreshold, DeviceKind, DeviceProperties, Technology, UPowerProxy,
    WarningLevel,
};
use tokio::sync::{oneshot, watch};
use zbus::Connection;
use zbus::fdo::{DBusProxy, PropertiesProxy};
use zbus::message::Type;
use zbus::{MatchRule, MessageStream};

use crate::{
    CommandError, Ctx, Input, NoConfig, Publisher, Service, ServiceEndpoint, ServiceError, Sub, say,
};

type Events = Pin<Box<dyn Stream<Item = Event> + Send>>;

const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatteryState {
    pub on_battery: bool,
    pub display: Option<Charge>,
    pub internals: Vec<Supply>,
    pub devices: Vec<Peripheral>,
    pub profile: Option<Profiles>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Charge {
    pub percentage: u8,
    pub state: ChargeState,
    pub icon_name: String,
    pub time_to_empty: Option<u32>,
    pub time_to_full: Option<u32>,
    pub energy_rate_mw: Option<u32>,
    pub warning: WarningLevel,
}

impl Charge {
    pub fn icon_name(&self) -> String {
        if !self.icon_name.is_empty() {
            return self.icon_name.clone();
        }
        fallback_icon(self.percentage, self.state)
    }
}

fn fallback_icon(percentage: u8, state: ChargeState) -> String {
    let band = u32::from(percentage).min(100) / 10 * 10;
    match state {
        ChargeState::Full => "battery-full-charged-symbolic".to_owned(),
        ChargeState::Charging | ChargeState::PendingDischarge if band == 100 => {
            "battery-level-100-charged-symbolic".to_owned()
        }
        ChargeState::Charging | ChargeState::PendingDischarge => {
            format!("battery-level-{band}-charging-symbolic")
        }
        ChargeState::PendingCharge => format!("battery-level-{band}-plugged-in-symbolic"),
        ChargeState::Discharging | ChargeState::Empty | ChargeState::Unknown => {
            format!("battery-level-{band}-symbolic")
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Supply {
    pub path: String,
    pub charge: Charge,
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub native_path: Option<String>,
    pub energy_mwh: Option<u32>,
    pub energy_full_mwh: Option<u32>,
    pub energy_full_design_mwh: Option<u32>,
    pub capacity_pct: Option<u8>,
    pub voltage_mv: Option<u32>,
    pub cycles: Option<u32>,
    pub technology: Technology,
    pub charge_threshold: Option<ChargeThreshold>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peripheral {
    pub path: String,
    pub kind: DeviceKind,
    pub name: String,
    pub icon_name: Option<String>,
    pub charge: Charge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profiles {
    pub active: String,
    pub available: Vec<String>,
    pub performance_degraded: Option<String>,
}

pub enum Command {
    SetProfile {
        name: String,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
    EnableChargeThreshold {
        path: String,
        enabled: bool,
        reply: oneshot::Sender<Result<(), CommandError>>,
    },
}

pub enum Event {
    Upower(UpowerSnapshot),
    Profiles(Option<Profiles>),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpowerSnapshot {
    on_battery: bool,
    display_path: Option<String>,
    devices: BTreeMap<String, DeviceProperties>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Watch {
    Upower,
    Profiles,
}

pub struct Battery {
    state: Publisher<BatteryState>,
    upower: UpowerSnapshot,
    profile: Option<Profiles>,
}

#[derive(Clone)]
pub struct BatteryHandle(ServiceEndpoint<Battery>);

impl BatteryHandle {
    pub fn snapshot(&self) -> BatteryState {
        self.0.snapshot()
    }

    pub fn subscribe(&self) -> watch::Receiver<BatteryState> {
        self.0.subscribe()
    }

    pub fn health(&self) -> watch::Receiver<crate::ServiceState> {
        self.0.health()
    }

    pub async fn set_profile(&self, name: impl Into<String>) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(Command::SetProfile {
            name: name.into(),
            reply,
        })?;
        result.await.map_err(|_| {
            CommandError::Unavailable("battery stopped before setting the profile".to_owned())
        })?
    }

    pub async fn enable_charge_threshold(
        &self,
        path: impl Into<String>,
        enabled: bool,
    ) -> Result<(), CommandError> {
        let (reply, result) = oneshot::channel();
        self.0.command(Command::EnableChargeThreshold {
            path: path.into(),
            enabled,
            reply,
        })?;
        result.await.map_err(|_| {
            CommandError::Unavailable(
                "battery stopped before changing the charge threshold".to_owned(),
            )
        })?
    }
}

impl Service for Battery {
    const NAME: &'static str = "battery";
    type Config = NoConfig;
    type State = BatteryState;
    type Handle = BatteryHandle;
    type Command = Command;
    type Event = Event;
    type Dependencies = ();
    type SubKey = Watch;

    fn from_endpoint(endpoint: ServiceEndpoint<Self>) -> Self::Handle {
        BatteryHandle(endpoint)
    }

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        vec![
            Sub::stream(Watch::Upower, upower_events),
            Sub::stream(Watch::Profiles, profile_events),
        ]
    }

    async fn start(
        ctx: &Ctx<Self>,
        _config: Self::Config,
        _dependencies: Self::Dependencies,
    ) -> Result<Self, ServiceError> {
        Ok(Self {
            state: ctx.publisher(),
            upower: UpowerSnapshot {
                on_battery: false,
                display_path: None,
                devices: BTreeMap::new(),
            },
            profile: None,
        })
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Event(Event::Upower(snapshot)) => {
                self.upower = snapshot;
                ctx.running();
                self.publish();
            }
            Input::Event(Event::Profiles(profile)) => {
                self.profile = profile;
                self.publish();
            }
            Input::Event(Event::Failed(reason)) => {
                ctx.degraded(reason);
            }
            Input::Command(Command::SetProfile { name, reply }) => {
                self.set_profile(ctx, name, reply);
            }
            Input::Command(Command::EnableChargeThreshold {
                path,
                enabled,
                reply,
            }) => {
                self.enable_threshold(ctx, path, enabled, reply);
            }
            Input::Config(_) => {}
        }
    }
}

impl Battery {
    fn publish(&self) {
        let (display, internals, devices) =
            assemble(&self.upower.devices, self.upower.display_path.as_deref());
        self.state.set(BatteryState {
            on_battery: self.upower.on_battery,
            display,
            internals,
            devices,
            profile: self.profile.clone(),
        });
    }

    fn set_profile(
        &self,
        ctx: &Ctx<Self>,
        name: String,
        reply: oneshot::Sender<Result<(), CommandError>>,
    ) {
        let Ok(bus) = ctx.system_bus().cloned() else {
            let _ = reply.send(Err(CommandError::Unavailable(
                "no system bus for power profiles".to_owned(),
            )));
            return;
        };
        ctx.spawn_detached(move |_ctx| async move {
            let _ = reply.send(set_active_profile(&bus, &name).await);
        });
    }

    fn enable_threshold(
        &self,
        ctx: &Ctx<Self>,
        path: String,
        enabled: bool,
        reply: oneshot::Sender<Result<(), CommandError>>,
    ) {
        let Ok(bus) = ctx.system_bus().cloned() else {
            let _ = reply.send(Err(CommandError::Unavailable(
                "no system bus for UPower".to_owned(),
            )));
            return;
        };
        ctx.spawn_detached(move |_ctx| async move {
            let _ = reply.send(enable_charge_threshold(&bus, &path, enabled).await);
        });
    }
}

pub fn assemble(
    devices: &BTreeMap<String, DeviceProperties>,
    display_path: Option<&str>,
) -> (Option<Charge>, Vec<Supply>, Vec<Peripheral>) {
    let mut display = None;
    let mut internals = Vec::new();
    let mut peripherals = Vec::new();

    for (path, properties) in devices {
        let composite = display_path == Some(path) || upower::is_display_device(path);
        if composite {
            if properties.is_present {
                display = Some(charge_of(properties));
            }
            continue;
        }
        if properties.kind.is_line_power() {
            continue;
        }
        if properties.power_supply && properties.kind.is_internal_supply() && properties.is_present
        {
            internals.push(supply_of(path, properties));
            continue;
        }
        if properties.is_present {
            peripherals.push(peripheral_of(path, properties));
        }
    }

    (display, internals, peripherals)
}

fn charge_of(properties: &DeviceProperties) -> Charge {
    Charge {
        percentage: properties.percentage,
        state: properties.state,
        icon_name: properties.icon_name.clone().unwrap_or_default(),
        time_to_empty: properties.time_to_empty,
        time_to_full: properties.time_to_full,
        energy_rate_mw: properties.energy_rate_mw,
        warning: properties.warning,
    }
}

fn supply_of(path: &str, properties: &DeviceProperties) -> Supply {
    Supply {
        path: path.to_owned(),
        charge: charge_of(properties),
        vendor: properties.vendor.clone(),
        model: properties.model.clone(),
        native_path: properties.native_path.clone(),
        energy_mwh: properties.energy_mwh,
        energy_full_mwh: properties.energy_full_mwh,
        energy_full_design_mwh: properties.energy_full_design_mwh,
        capacity_pct: properties.capacity_pct,
        voltage_mv: properties.voltage_mv,
        cycles: properties.cycles,
        technology: properties.technology,
        charge_threshold: properties.charge_threshold,
    }
}

fn peripheral_of(path: &str, properties: &DeviceProperties) -> Peripheral {
    Peripheral {
        path: path.to_owned(),
        kind: properties.kind,
        name: properties
            .model
            .clone()
            .or_else(|| properties.vendor.clone())
            .or_else(|| properties.native_path.clone())
            .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path).to_owned()),
        icon_name: properties.icon_name.clone(),
        charge: charge_of(properties),
    }
}

async fn upower_events(ctx: Ctx<Battery>) -> Events {
    let Ok(bus) = ctx.system_bus().cloned() else {
        return Box::pin(stream::once(async {
            Event::Failed("no system bus for UPower".to_owned())
        }));
    };
    let proxy = match UPowerProxy::new(&bus).await {
        Ok(proxy) => proxy,
        Err(error) => {
            let error = say(error);
            return Box::pin(stream::once(async move { Event::Failed(error) }));
        }
    };

    let mut wakes: Vec<Pin<Box<dyn Stream<Item = ()> + Send>>> = Vec::new();
    if let Ok(added) = proxy.receive_device_added().await {
        wakes.push(wake(added));
    }
    if let Ok(removed) = proxy.receive_device_removed().await {
        wakes.push(wake(removed));
    }
    wakes.push(wake(proxy.receive_on_battery_changed().await));
    if let Some(changed) = device_properties(&bus).await {
        wakes.push(Box::pin(changed.map(|_| ())));
    }
    if let Ok(dbus) = DBusProxy::new(&bus).await
        && let Ok(owner) = dbus.receive_name_owner_changed().await
    {
        wakes.push(Box::pin(owner.filter_map(|signal| async move {
            let args = signal.args().ok()?;
            (args.name.as_str() == upower::SERVICE).then_some(())
        })));
    }

    let first = snapshot_upower(&bus).await;
    Box::pin(
        stream::once(async move { first })
            .chain(stream::select_all(wakes).then({
                let bus = bus.clone();
                move |_| {
                    let bus = bus.clone();
                    async move { snapshot_upower(&bus).await }
                }
            }))
            .map(|result| match result {
                Ok(snapshot) => Event::Upower(snapshot),
                Err(error) => Event::Failed(error),
            }),
    )
}

async fn profile_events(ctx: Ctx<Battery>) -> Events {
    let Ok(bus) = ctx.system_bus().cloned() else {
        return Box::pin(stream::once(async { Event::Profiles(None) }));
    };

    let mut wakes: Vec<Pin<Box<dyn Stream<Item = ()> + Send>>> = Vec::new();
    if let Ok(proxy) = PowerProfilesDaemonProxy::new(&bus).await {
        wakes.push(wake(proxy.receive_active_profile_changed().await));
        wakes.push(wake(proxy.receive_profiles_changed().await));
        wakes.push(wake(proxy.receive_performance_degraded_changed().await));
    }
    if let Ok(dbus) = DBusProxy::new(&bus).await
        && let Ok(owner) = dbus.receive_name_owner_changed().await
    {
        wakes.push(Box::pin(owner.filter_map(|signal| async move {
            let args = signal.args().ok()?;
            (args.name.as_str() == power_profiles::SERVICE).then_some(())
        })));
    }

    let first = profiles(&bus).await;
    Box::pin(stream::once(async move { Event::Profiles(first) }).chain(
        stream::select_all(wakes).then({
            let bus = bus.clone();
            move |_| {
                let bus = bus.clone();
                async move { Event::Profiles(profiles(&bus).await) }
            }
        }),
    ))
}

fn wake<S>(stream: S) -> Pin<Box<dyn Stream<Item = ()> + Send>>
where
    S: Stream + Send + 'static,
{
    Box::pin(stream.map(|_| ()))
}

async fn device_properties(bus: &Connection) -> Option<MessageStream> {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender(upower::SERVICE)
        .ok()?
        .interface(PROPERTIES)
        .ok()?
        .member("PropertiesChanged")
        .ok()?
        .path_namespace(upower::DEVICES)
        .ok()?
        .build();
    MessageStream::for_match_rule(rule, bus, None).await.ok()
}

async fn snapshot_upower(bus: &Connection) -> Result<UpowerSnapshot, String> {
    let proxy = UPowerProxy::new(bus).await.map_err(say)?;
    let on_battery = proxy.on_battery().await.map_err(say)?;
    let display = proxy.get_display_device().await.ok();
    let display_path = display.as_ref().map(|path| path.as_str().to_owned());
    let mut paths = proxy.enumerate_devices().await.map_err(say)?;
    if let Some(display) = display
        && !paths.iter().any(|path| path.as_str() == display.as_str())
    {
        paths.push(display);
    }

    let mut devices = BTreeMap::new();
    for path in paths {
        let Some(properties) = device_at(bus, path.as_str()).await else {
            continue;
        };
        devices.insert(path.as_str().to_owned(), properties);
    }
    Ok(UpowerSnapshot {
        on_battery,
        display_path,
        devices,
    })
}

async fn device_at(bus: &Connection, path: &str) -> Option<DeviceProperties> {
    let proxy = PropertiesProxy::builder(bus)
        .destination(upower::SERVICE)
        .ok()?
        .path(path)
        .ok()?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .ok()?;
    let interface = zbus::names::InterfaceName::try_from(upower::DEVICE_INTERFACE).ok()?;
    let map = proxy.get_all(interface).await.ok()?;
    Some(upower::decode_device(&map))
}

async fn profiles(bus: &Connection) -> Option<Profiles> {
    let dbus = DBusProxy::new(bus).await.ok()?;
    let name = zbus::names::BusName::try_from(power_profiles::SERVICE).ok()?;
    if !dbus.name_has_owner(name).await.ok()? {
        return None;
    }
    let proxy = PowerProfilesDaemonProxy::new(bus).await.ok()?;
    let active = proxy.active_profile().await.ok()?;
    let available = power_profiles::decode_profile_names(&proxy.profiles().await.ok()?);
    if available.is_empty() {
        return None;
    }
    let degraded = proxy
        .performance_degraded()
        .await
        .ok()
        .and_then(|reason| (!reason.is_empty()).then_some(reason));
    Some(Profiles {
        active,
        available,
        performance_degraded: degraded,
    })
}

async fn set_active_profile(bus: &Connection, name: &str) -> Result<(), CommandError> {
    let proxy = PowerProfilesDaemonProxy::new(bus)
        .await
        .map_err(|error| CommandError::Unavailable(say(error)))?;
    proxy
        .set_active_profile(name)
        .await
        .map_err(|error| CommandError::Internal(say(error)))
}

async fn enable_charge_threshold(
    bus: &Connection,
    path: &str,
    enabled: bool,
) -> Result<(), CommandError> {
    let proxy = upower::UPowerDeviceProxy::builder(bus)
        .path(path)
        .map_err(|error| CommandError::Internal(say(error)))?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .map_err(|error| CommandError::Unavailable(say(error)))?;
    proxy
        .enable_charge_threshold(enabled)
        .await
        .map_err(|error| CommandError::Internal(say(error)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_dbus::{Buses, upower::ChargeState};
    use tokio_util::sync::CancellationToken;

    use crate::ServiceRuntime;

    fn battery(
        path: &str,
        present: bool,
        percentage: u8,
        power_supply: bool,
    ) -> (String, DeviceProperties) {
        (
            path.to_owned(),
            DeviceProperties {
                native_path: Some("BAT1".to_owned()),
                vendor: Some("ASUS".to_owned()),
                model: Some("A32-K55".to_owned()),
                kind: DeviceKind::Battery,
                power_supply,
                is_present: present,
                percentage,
                state: ChargeState::Full,
                icon_name: Some("battery-full-charged-symbolic".to_owned()),
                capacity_pct: Some(97),
                energy_mwh: Some(87_042),
                energy_full_mwh: Some(87_042),
                energy_full_design_mwh: Some(90_005),
                voltage_mv: Some(17_243),
                charge_threshold: Some(ChargeThreshold {
                    enabled: false,
                    start: Some(75),
                    end: Some(80),
                }),
                ..DeviceProperties::default()
            },
        )
    }

    fn charge(percentage: u8, state: ChargeState) -> Charge {
        Charge {
            percentage,
            state,
            icon_name: "battery-full-charged-symbolic".to_owned(),
            time_to_empty: None,
            time_to_full: None,
            energy_rate_mw: None,
            warning: WarningLevel::None,
        }
    }

    #[test]
    fn the_chip_uses_upower_icon_name_when_it_is_set() {
        let mut charging = charge(40, ChargeState::Charging);
        charging.icon_name = "battery-full-charging-symbolic".to_owned();
        assert_eq!(charging.icon_name(), "battery-full-charging-symbolic");
        charging.icon_name.clear();
        assert_eq!(charging.icon_name(), "battery-level-40-charging-symbolic");

        let mut full = charge(100, ChargeState::Charging);
        full.icon_name.clear();
        assert_eq!(
            full.icon_name(),
            "battery-level-100-charged-symbolic",
            "Adwaita has no battery-level-100-charging-symbolic"
        );

        let mut waiting = charge(55, ChargeState::PendingCharge);
        waiting.icon_name.clear();
        assert_eq!(waiting.icon_name(), "battery-level-50-plugged-in-symbolic");
    }

    #[test]
    fn display_device_is_the_chip_and_not_an_internal_row() {
        let devices = BTreeMap::from([
            battery(upower::DISPLAY_DEVICE, true, 100, true),
            battery(
                "/org/freedesktop/UPower/devices/battery_BAT1",
                true,
                100,
                true,
            ),
            (
                "/org/freedesktop/UPower/devices/line_power_ACAD".to_owned(),
                DeviceProperties {
                    kind: DeviceKind::LinePower,
                    online: true,
                    icon_name: Some("ac-adapter-symbolic".to_owned()),
                    ..DeviceProperties::default()
                },
            ),
        ]);

        let (display, internals, peripherals) = assemble(&devices, Some(upower::DISPLAY_DEVICE));
        assert_eq!(display.unwrap().percentage, 100);
        assert_eq!(internals.len(), 1);
        assert_eq!(internals[0].model.as_deref(), Some("A32-K55"));
        assert!(
            internals[0].charge_threshold.is_some(),
            "details come from BAT1, which supports a charge limit"
        );
        assert!(peripherals.is_empty());
    }

    #[test]
    fn a_missing_display_device_hides_the_chip() {
        let devices = BTreeMap::from([battery(
            "/org/freedesktop/UPower/devices/battery_BAT1",
            true,
            80,
            true,
        )]);
        let (display, internals, _) = assemble(&devices, None);
        assert!(display.is_none(), "the chip reads DisplayDevice only");
        assert_eq!(internals.len(), 1);
    }

    #[test]
    fn an_absent_display_device_is_no_chip() {
        let devices = BTreeMap::from([battery(upower::DISPLAY_DEVICE, false, 0, true)]);
        let (display, internals, _) = assemble(&devices, None);
        assert!(display.is_none());
        assert!(internals.is_empty());
    }

    #[test]
    fn a_mouse_is_a_peripheral_and_not_an_internal_supply() {
        let devices = BTreeMap::from([(
            "/org/freedesktop/UPower/devices/mouse_hidpp".to_owned(),
            DeviceProperties {
                kind: DeviceKind::Mouse,
                is_present: true,
                power_supply: false,
                model: Some("MX Master 3S".to_owned()),
                percentage: 41,
                state: ChargeState::Discharging,
                icon_name: Some("input-mouse-symbolic".to_owned()),
                ..DeviceProperties::default()
            },
        )]);
        let (display, internals, peripherals) = assemble(&devices, Some(upower::DISPLAY_DEVICE));
        assert!(display.is_none() && internals.is_empty());
        assert_eq!(peripherals.len(), 1);
        assert_eq!(peripherals[0].name, "MX Master 3S");
        assert_eq!(peripherals[0].charge.percentage, 41);
    }

    #[tokio::test]
    async fn no_system_bus_degrades_and_shows_nothing() {
        let cancel = CancellationToken::new();
        let (mut runtime, handle) = ServiceRuntime::<Battery>::new(
            NoConfig,
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        );
        let running = tokio::spawn(async move { runtime.run(()).await });
        for _ in 0..16 {
            tokio::task::yield_now().await;
        }
        let snapshot = handle.snapshot();
        assert!(snapshot.display.is_none());
        assert!(snapshot.internals.is_empty());
        assert!(snapshot.profile.is_none());
        assert!(
            handle.health().borrow().unavailable_reason().is_some(),
            "a missing bus is degraded, not a silent empty bar"
        );
        cancel.cancel();
        let _ = running.await;
    }

    #[tokio::test]
    async fn a_profile_command_without_a_bus_is_unavailable() {
        let cancel = CancellationToken::new();
        let (mut runtime, handle) = ServiceRuntime::<Battery>::new(
            NoConfig,
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        );
        let running = tokio::spawn(async move { runtime.run(()).await });
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        let error = handle
            .set_profile("power-saver")
            .await
            .expect_err("there is no daemon to write to");
        assert!(matches!(error, CommandError::Unavailable(_)));
        cancel.cancel();
        let _ = running.await;
    }
}
