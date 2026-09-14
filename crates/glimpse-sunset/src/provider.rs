use glimpse_config::Schedule;
use glimpse_dbus::night_light::{
    GLIMPSE_NIGHT_LIGHT_BUS_NAME, GLIMPSE_NIGHT_LIGHT_OBJECT_PATH, NightLightSnapshot,
};
use glimpse_services::{NightLightHandle, ServiceState};
use tokio::task::JoinHandle;
use zbus::Connection;
use zbus::fdo::{RequestNameFlags, RequestNameReply};

/// Read-only: the schedule lives in `[night-light]`, which is already re-read on change, so a
/// setter here would either write the user's document or invent a runtime override nothing asked
/// for.
struct Provider {
    night_light: NightLightHandle,
}

#[zbus::interface(name = "me.aresa.Glimpse.NightLight1")]
impl Provider {
    #[zbus(property)]
    fn snapshot(&self) -> NightLightSnapshot {
        snapshot(&self.night_light)
    }
}

pub struct Runtime {
    connection: Connection,
    changes: JoinHandle<()>,
}

impl Runtime {
    /// Exported first and named second, which is the order zbus asks for: a `Get` arriving between
    /// the two would otherwise find the name but no object.
    ///
    /// The caller runs this before touching any backend, so a second copy of this binary fails
    /// here rather than after taking gamma control from the one already running. The flags are the
    /// other half: plain `request_name` asks for `AllowReplacement`, and a duplicate then silently
    /// steals the name, leaving the first process applying gamma and unreachable on D-Bus —
    /// measured, not feared.
    pub async fn start(
        connection: Connection,
        night_light: NightLightHandle,
    ) -> zbus::Result<Self> {
        connection
            .object_server()
            .at(
                GLIMPSE_NIGHT_LIGHT_OBJECT_PATH,
                Provider {
                    night_light: night_light.clone(),
                },
            )
            .await?;

        let named = connection
            .request_name_with_flags(
                GLIMPSE_NIGHT_LIGHT_BUS_NAME,
                RequestNameFlags::DoNotQueue.into(),
            )
            .await;
        match named {
            Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => {}
            other => {
                let _ = connection
                    .object_server()
                    .remove::<Provider, _>(GLIMPSE_NIGHT_LIGHT_OBJECT_PATH)
                    .await;
                return Err(other.err().unwrap_or(zbus::Error::NameTaken));
            }
        }
        tracing::info!(
            bus_name = GLIMPSE_NIGHT_LIGHT_BUS_NAME,
            "night light D-Bus name acquired"
        );

        let changes = tokio::spawn(follow_changes(connection.clone(), night_light));
        Ok(Self {
            connection,
            changes,
        })
    }
    pub fn cancel(&self) {
        self.changes.abort();
    }

    pub async fn shutdown(self) {
        let Self {
            connection,
            changes,
        } = self;
        changes.abort();
        let _ = changes.await;
        if let Err(error) = connection.release_name(GLIMPSE_NIGHT_LIGHT_BUS_NAME).await {
            tracing::warn!(%error, "night light D-Bus name release failed");
        }
        if let Err(error) = connection
            .object_server()
            .remove::<Provider, _>(GLIMPSE_NIGHT_LIGHT_OBJECT_PATH)
            .await
        {
            tracing::warn!(%error, "night light D-Bus object removal failed");
        }
        tracing::info!("night light D-Bus provider stopped");
    }
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
    let (serving, reason) = availability(&health.borrow());
    NightLightSnapshot {
        schedule: schedule(state.schedule).to_owned(),
        temperature: state.temperature,
        target: state.target,
        active: state.active(),
        serving,
        reason,
    }
}

fn schedule(schedule: Schedule) -> &'static str {
    match schedule {
        Schedule::Off => "off",
        Schedule::Automatic => "automatic",
        Schedule::Schedule => "schedule",
    }
}

fn availability(state: &ServiceState) -> (bool, String) {
    match state {
        ServiceState::Running => (true, String::new()),
        ServiceState::Starting => (false, "starting".to_owned()),
        ServiceState::Degraded { reason } => (false, reason.clone()),
        ServiceState::Stopped { reason } => (
            false,
            reason.clone().unwrap_or_else(|| "stopped".to_owned()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::io::BufRead as _;
    use std::process::{Child, Command, Stdio};

    use glimpse_dbus::{Buses, night_light::NightLight1Proxy};
    use glimpse_services::{
        FakeGamma, NightLight, NightLightConfig, NightLightDependencies, Service, ServiceRuntime,
        Solar, initial_night_light_state,
    };
    use tokio_util::sync::CancellationToken;

    use super::*;

    #[test]
    fn every_schedule_has_a_wire_spelling_matching_the_document() {
        assert_eq!(schedule(Schedule::Off), "off");
        assert_eq!(schedule(Schedule::Automatic), "automatic");
        assert_eq!(schedule(Schedule::Schedule), "schedule");
    }

    #[test]
    fn availability_distinguishes_serving_from_the_reason_it_is_not() {
        assert_eq!(availability(&ServiceState::Running), (true, String::new()));
        assert_eq!(
            availability(&ServiceState::Degraded {
                reason: "another gamma client holds the outputs".to_owned()
            }),
            (false, "another gamma client holds the outputs".to_owned())
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
            Solar::initial_state(),
            buses.clone(),
            cancel.child_token(),
        );
        let solar_task = tokio::spawn(async move {
            solar_runtime
                .run(
                    <Solar as Service>::Config::from(&glimpse_config::Config::default()),
                    glimpse_services::SolarDependencies {
                        geolocation: {
                            let (runtime, handle) =
                                ServiceRuntime::<glimpse_services::Geolocation>::new(
                                    glimpse_services::Geolocation::initial_state(),
                                    Buses::unavailable("no backend bus in test"),
                                    CancellationToken::new(),
                                );
                            drop(runtime);
                            handle
                        },
                    },
                )
                .await
        });

        let config = NightLightConfig::from(&glimpse_config::Config::default());
        let (mut night_runtime, night_light) = ServiceRuntime::<NightLight>::new(
            initial_night_light_state(&config),
            buses,
            cancel.child_token(),
        );
        let night_task = tokio::spawn(async move {
            night_runtime
                .run(
                    config,
                    NightLightDependencies {
                        solar,
                        gamma: Box::new(FakeGamma::default()),
                    },
                )
                .await
        });

        let provider = Runtime::start(bus.connection().await, night_light)
            .await
            .expect("the provider starts");
        let client = bus.connection().await;
        let proxy = NightLight1Proxy::new(&client).await.expect("a proxy");

        let snapshot = proxy.snapshot().await.expect("a snapshot");
        assert_eq!(snapshot.schedule, "automatic");
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
    <property name="Snapshot" type="(suubbs)" access="read"/>
  </interface>"#
        );

        provider.shutdown().await;
        cancel.cancel();
        let _ = night_task.await;
        let _ = solar_task.await;
    }
}
