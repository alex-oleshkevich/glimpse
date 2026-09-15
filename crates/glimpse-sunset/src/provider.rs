use glimpse_config::Schedule;
use glimpse_dbus::Exported;
use glimpse_dbus::night_light::{
    GLIMPSE_NIGHT_LIGHT_BUS_NAME, GLIMPSE_NIGHT_LIGHT_OBJECT_PATH, NightLightSnapshot,
};
use glimpse_services::NightLightHandle;
use zbus::{Connection, DBusError};

#[derive(Debug, DBusError)]
#[zbus(prefix = "me.aresa.Glimpse.NightLight1.Error", impl_display = true)]
pub enum Error {
    InvalidSchedule(String),
    Unavailable(String),
    #[zbus(error)]
    ZBus(zbus::Error),
}

pub(crate) struct Provider {
    night_light: NightLightHandle,
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
            .map_err(|error| Error::Unavailable(error.to_string()))
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
    Exported::start(
        connection.clone(),
        GLIMPSE_NIGHT_LIGHT_BUS_NAME,
        GLIMPSE_NIGHT_LIGHT_OBJECT_PATH,
        Provider {
            night_light: night_light.clone(),
        },
        follow_changes(connection, night_light),
    )
    .await
}

async fn follow_changes(connection: Connection, night_light: NightLightHandle) {
    let mut state = night_light.subscribe();
    let mut health = night_light.health();
    loop {
        tokio::select! {
            changed = state.changed() => if changed.is_err() { return },
            changed = health.changed() => if changed.is_err() { return },
        }

        let interface = match connection
            .object_server()
            .interface::<_, Provider>(GLIMPSE_NIGHT_LIGHT_OBJECT_PATH)
            .await
        {
            Ok(interface) => interface,
            Err(error) => {
                tracing::error!(%error, "night light provider object disappeared");
                return;
            }
        };
        if let Err(error) = interface
            .get()
            .await
            .snapshot_changed(interface.signal_emitter())
            .await
        {
            tracing::warn!(%error, "night light snapshot change signal failed");
        }
    }
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
    }
}

#[cfg(test)]
mod tests {
    use std::io::BufRead as _;
    use std::process::{Child, Command, Stdio};

    use glimpse_dbus::{Buses, night_light::NightLight1Proxy};
    use glimpse_services::{
        FakeGamma, NightLight, NightLightConfig, NightLightDependencies, Service, ServiceRuntime,
        Solar,
    };
    use tokio_util::sync::CancellationToken;
    use zbus::proxy::CacheProperties;

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

    struct PrivateBus {
        child: Child,
        address: String,
    }

    impl PrivateBus {
        fn start() -> Self {
            let mut child = Command::new("dbus-daemon")
                .args([
                    "--session",
                    "--nofork",
                    "--print-address=1",
                    "--print-pid=1",
                ])
                .stdout(Stdio::piped())
                .spawn()
                .expect("dbus-daemon starts");
            let stdout = child.stdout.as_mut().expect("a pipe");
            let mut lines = std::io::BufReader::new(stdout).lines();
            let address = lines.next().expect("an address").expect("readable");
            let _pid = lines.next().expect("a pid").expect("readable");
            Self { child, address }
        }

        async fn connection(&self) -> Connection {
            zbus::connection::Builder::address(self.address.as_str())
                .expect("a valid address")
                .build()
                .await
                .expect("a connection")
        }
    }

    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
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
        assert_eq!(snapshot.temperature, 6500);
        assert_eq!(snapshot.target, 4200);
        assert!(!snapshot.active);

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
    <property name="Snapshot" type="(sbuubbs)" access="read"/>
  </interface>"#
        );

        proxy
            .set_schedule("off")
            .await
            .expect("a known mode is accepted");
        let overridden = proxy.snapshot().await.expect("a snapshot");
        assert_eq!(overridden.schedule, "off");
        assert!(overridden.overridden, "the document still says automatic");

        proxy
            .set_schedule("sometimes")
            .await
            .expect_err("an unknown mode is refused");

        provider.shutdown().await;
        cancel.cancel();
        let _ = night_task.await;
        let _ = solar_task.await;
    }
}
