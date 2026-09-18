use glimpse_config::Schedule;
use glimpse_dbus::night_light::{
    GLIMPSE_NIGHT_LIGHT_BUS_NAME, GLIMPSE_NIGHT_LIGHT_OBJECT_PATH, NightLightSnapshot,
};
use glimpse_dbus::{Exported, Snapshot};
use glimpse_services::{CommandError, NightLightHandle};
use zbus::{Connection, DBusError};

#[derive(Debug, DBusError)]
#[zbus(prefix = "me.aresa.Glimpse.NightLight1.Error", impl_display = true)]
pub enum Error {
    InvalidSchedule(String),
    InvalidTemperature(String),
    Unavailable(String),
    #[zbus(error)]
    ZBus(zbus::Error),
}

impl From<CommandError> for Error {
    fn from(error: CommandError) -> Self {
        match error {
            CommandError::InvalidArgument(reason) => Self::InvalidTemperature(reason),
            error => Self::Unavailable(error.to_string()),
        }
    }
}

pub(crate) struct Provider {
    night_light: NightLightHandle,
}

impl Snapshot for Provider {
    async fn emit_snapshot_changed(
        &self,
        emitter: &zbus::object_server::SignalEmitter<'_>,
    ) -> zbus::Result<()> {
        self.snapshot_changed(emitter).await
    }
}

#[zbus::interface(name = "me.aresa.Glimpse.NightLight1")]
impl Provider {
    #[zbus(property)]
    fn snapshot(&self) -> NightLightSnapshot {
        snapshot(&self.night_light)
    }

    async fn set_schedule(&self, schedule: &str) -> Result<(), Error> {
        self.night_light
            .set_schedule(parse(schedule)?)
            .await
            .map_err(Error::from)
    }

    async fn set_temperature(&self, kelvin: u32) -> Result<(), Error> {
        self.night_light
            .set_temperature(if kelvin == 0 { None } else { Some(kelvin) })
            .await
            .map_err(Error::from)
    }
}

fn parse(schedule: &str) -> Result<Schedule, Error> {
    Schedule::parse(schedule)
        .ok_or_else(|| Error::InvalidSchedule("schedule is off, automatic or schedule".to_owned()))
}

pub(crate) type Runtime = Exported<Provider>;

pub(crate) async fn start(
    connection: Connection,
    night_light: NightLightHandle,
) -> zbus::Result<Runtime> {
    Exported::serve(
        connection,
        GLIMPSE_NIGHT_LIGHT_BUS_NAME,
        GLIMPSE_NIGHT_LIGHT_OBJECT_PATH,
        Provider {
            night_light: night_light.clone(),
        },
        night_light.subscribe(),
        night_light.health(),
    )
    .await
}

fn snapshot(night_light: &NightLightHandle) -> NightLightSnapshot {
    let state = night_light.snapshot();
    let health = night_light.health();
    let unavailable = health.borrow().unavailable_reason().map(str::to_owned);
    NightLightSnapshot {
        schedule: state.schedule.as_str().to_owned(),
        overridden: state.overridden,
        temperature: state.temperature,
        target: state.target,
        active: state.active(),
        serving: unavailable.is_none(),
        reason: unavailable.unwrap_or_default(),
        configured: state.configured.as_str().to_owned(),
        manual: state.manual,
    }
}

#[cfg(test)]
mod tests {

    use glimpse_dbus::{Buses, night_light::NightLight1Proxy};
    use glimpse_services::{
        FakeGamma, NightLight, NightLightConfig, NightLightDependencies, Service, ServiceRuntime,
        Solar,
    };
    use tokio_util::sync::CancellationToken;
    use zbus::proxy::CacheProperties;

    use glimpse_dbus::testing::PrivateBus;

    use super::*;

    #[test]
    fn every_spelling_the_snapshot_prints_is_one_set_schedule_accepts() {
        for mode in [Schedule::Off, Schedule::Automatic, Schedule::Schedule] {
            assert_eq!(parse(mode.as_str()).expect("a known spelling"), mode);
        }
    }

    #[test]
    fn an_unknown_schedule_is_refused_without_quoting_what_arrived() {
        let rejected = parse("<img src=x>").expect_err("not a mode");
        assert!(
            !rejected.to_string().contains("img"),
            "the rejected spelling must not be echoed, got {rejected}"
        );
    }

    #[test]
    fn a_snapshot_says_the_service_is_not_serving_and_why() {
        let (_runtime, night_light) = ServiceRuntime::<NightLight>::new(
            NightLightConfig::from(&glimpse_config::Config::default()),
            Buses::unavailable("no bus in tests"),
            CancellationToken::new(),
        );

        let published = snapshot(&night_light);

        assert!(!published.serving);
        assert_eq!(published.reason, "starting");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_refused_name_takes_the_exported_object_back_down() {
        let bus = PrivateBus::start();
        let holder = bus.connection().await;
        glimpse_dbus::own_name(&holder, GLIMPSE_NIGHT_LIGHT_BUS_NAME)
            .await
            .expect("the first owner takes the name");

        let (_runtime, night_light) = ServiceRuntime::<NightLight>::new(
            NightLightConfig::from(&glimpse_config::Config::default()),
            Buses::unavailable("no bus in tests"),
            CancellationToken::new(),
        );

        let connection = bus.connection().await;
        assert!(
            matches!(
                start(connection.clone(), night_light).await,
                Err(zbus::Error::NameTaken)
            ),
            "the name is held, so starting must fail"
        );

        assert!(
            matches!(
                connection
                    .object_server()
                    .remove::<Provider, _>(GLIMPSE_NIGHT_LIGHT_OBJECT_PATH)
                    .await,
                Err(zbus::Error::InterfaceNotFound)
            ),
            "a refused name must leave nothing exported: the object is put up before the name is \
             asked for, so the failure has to take it back down"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn introspection_and_the_typed_property_match_the_frozen_interface() {
        let bus = PrivateBus::start();
        let cancel = CancellationToken::new();
        let buses = Buses::unavailable("no backend bus in test");
        let (mut solar_runtime, solar) = ServiceRuntime::<Solar>::new(
            <Solar as Service>::Config::from(&glimpse_config::Config::default()),
            buses.clone(),
            cancel.child_token(),
        );
        let solar_task = tokio::spawn(async move {
            solar_runtime
                .run(glimpse_services::SolarDependencies {
                    geolocation: {
                        let (runtime, handle) =
                            ServiceRuntime::<glimpse_services::Geolocation>::new(
                                <glimpse_services::Geolocation as Service>::Config::from(
                                    &glimpse_config::Config::default(),
                                ),
                                Buses::unavailable("no backend bus in test"),
                                CancellationToken::new(),
                            );
                        drop(runtime);
                        handle
                    },
                })
                .await
        });

        let (mut night_runtime, night_light) = ServiceRuntime::<NightLight>::new(
            NightLightConfig::from(&glimpse_config::Config::default()),
            buses,
            cancel.child_token(),
        );
        let night_task = tokio::spawn(async move {
            night_runtime
                .run(NightLightDependencies {
                    solar,
                    gamma: Box::new(FakeGamma::default()),
                })
                .await
        });

        let provider = start(bus.connection().await, night_light)
            .await
            .expect("the provider starts");
        let client = bus.connection().await;
        let proxy = NightLight1Proxy::builder(&client)
            .cache_properties(CacheProperties::No)
            .build()
            .await
            .expect("a proxy");

        let snapshot = proxy.snapshot().await.expect("a snapshot");
        assert_eq!(snapshot.schedule, "automatic");
        assert!(!snapshot.overridden);
        assert_eq!(snapshot.configured, "automatic");
        assert_eq!(snapshot.temperature, 6500);
        assert_eq!(snapshot.target, 4200);
        assert!(!snapshot.active);
        assert!(!snapshot.manual);

        let reply = client
            .call_method(
                Some(GLIMPSE_NIGHT_LIGHT_BUS_NAME),
                GLIMPSE_NIGHT_LIGHT_OBJECT_PATH,
                Some("org.freedesktop.DBus.Introspectable"),
                "Introspect",
                &(),
            )
            .await
            .expect("introspection answers");
        let xml: String = reply.body().deserialize().expect("a document");
        let start = xml
            .find("<interface name=\"me.aresa.Glimpse.NightLight1\">")
            .expect("the interface is exported");
        let end =
            start + xml[start..].find("</interface>").expect("a close") + "</interface>".len();
        assert_eq!(
            &xml[start..end],
            r#"<interface name="me.aresa.Glimpse.NightLight1">
    <method name="SetSchedule">
      <arg name="schedule" type="s" direction="in"/>
    </method>
    <method name="SetTemperature">
      <arg name="kelvin" type="u" direction="in"/>
    </method>
    <property name="Snapshot" type="(sbuubbssb)" access="read"/>
  </interface>"#
        );

        proxy
            .set_schedule("off")
            .await
            .expect("a known mode is accepted");
        let overridden = proxy.snapshot().await.expect("a snapshot");
        assert_eq!(overridden.schedule, "off");
        assert!(overridden.overridden, "the document still says automatic");
        assert_eq!(
            overridden.configured, "automatic",
            "the document's own schedule survives the override, so the UI has something to \
             switch back to"
        );

        proxy
            .set_schedule("sometimes")
            .await
            .expect_err("an unknown mode is refused");

        provider.shutdown().await;
        cancel.cancel();
        let _ = night_task.await;
        let _ = solar_task.await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn set_temperature_refuses_out_of_range_and_sets_the_manual_flag_in_range() {
        let bus = PrivateBus::start();
        let cancel = CancellationToken::new();
        let buses = Buses::unavailable("no backend bus in test");

        let (solar_runtime, solar) = ServiceRuntime::<Solar>::new(
            <Solar as Service>::Config::from(&glimpse_config::Config::default()),
            buses.clone(),
            cancel.child_token(),
        );
        drop(solar_runtime);

        let mut document = glimpse_config::Config::default();
        document.night_light.schedule = Schedule::Schedule;
        document.night_light.start_time = Some("20:00".to_owned());
        document.night_light.end_time = Some("07:00".to_owned());

        let (mut night_runtime, night_light) = ServiceRuntime::<NightLight>::new(
            NightLightConfig::from(&document),
            buses,
            cancel.child_token(),
        );
        let night_task = tokio::spawn(async move {
            night_runtime
                .run(NightLightDependencies {
                    solar,
                    gamma: Box::new(FakeGamma::default()),
                })
                .await
        });

        let provider = start(bus.connection().await, night_light)
            .await
            .expect("the provider starts");
        let client = bus.connection().await;
        let proxy = NightLight1Proxy::builder(&client)
            .cache_properties(CacheProperties::No)
            .build()
            .await
            .expect("a proxy");

        let refused = proxy
            .set_temperature(999)
            .await
            .expect_err("999 is below the accepted range");
        match &refused {
            zbus::Error::MethodError(name, detail, _) => {
                assert_eq!(
                    name.as_str(),
                    "me.aresa.Glimpse.NightLight1.Error.InvalidTemperature"
                );
                let detail = detail.clone().unwrap_or_default();
                assert!(
                    !detail.contains("999"),
                    "the rejected value must not be echoed, got {detail}"
                );
            }
            other => panic!("expected a MethodError, got {other:?}"),
        }

        proxy
            .set_temperature(3000)
            .await
            .expect("3000 is in range and any Schedule window has a boundary");
        let manual = proxy.snapshot().await.expect("a snapshot");
        assert!(manual.manual);
        assert!(
            !manual.overridden,
            "a manual temperature does not change what overridden means"
        );

        proxy
            .set_temperature(0)
            .await
            .expect("zero clears the override and is always accepted");
        let cleared = proxy.snapshot().await.expect("a snapshot");
        assert!(!cleared.manual);

        provider.shutdown().await;
        cancel.cancel();
        let _ = night_task.await;
    }
}
