use anyhow::{Context as _, Result};
use glimpse_config::Config;
use glimpse_dbus::Buses;
use glimpse_services::{
    Geolocation, NightLight, NightLightConfig, NightLightDependencies, Service, ServiceRuntime,
    ServiceSender, Solar, SolarDependencies,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::gamma::WaylandGamma;
use crate::provider;

pub struct SunsetServices {
    location: ServiceSender<Geolocation>,
    night: ServiceSender<NightLight>,
    /// Stop order is the reverse of start order, so the night light hands the outputs back while
    /// everything it reads is still alive.
    started: Vec<(&'static str, JoinHandle<()>)>,
    cancel: CancellationToken,
    /// `Option` only so `shutdown` can move it out of a type that also implements `Drop`.
    provider: Option<provider::Runtime>,
}

impl SunsetServices {
    /// The order is the whole of this function. Building a `ServiceRuntime` allocates channels and
    /// nothing else, so the handles exist before anything has a side effect; the D-Bus object and
    /// name are taken next, which is where a second copy of this binary fails; only then is gamma
    /// control taken and the services actually started. A duplicate therefore never touches the
    /// outputs the running one is holding.
    pub async fn start(document: &Config) -> Result<Self> {
        let buses = Buses::connect().await;
        let session = buses
            .session_bus()
            .cloned()
            .map_err(|reason| anyhow::anyhow!(reason.to_owned()))
            .context("the night light provider needs the session bus")?;

        let cancel = CancellationToken::new();
        let (location_runtime, location) = ServiceRuntime::<Geolocation>::new(
            <Geolocation as Service>::Config::from(document),
            buses.clone(),
            cancel.child_token(),
        );
        let (solar_runtime, solar) = ServiceRuntime::<Solar>::new(
            <Solar as Service>::Config::from(document),
            buses.clone(),
            cancel.child_token(),
        );
        let (night_runtime, night_light) = ServiceRuntime::<NightLight>::new(
            NightLightConfig::from(document),
            buses,
            cancel.child_token(),
        );

        let provider = provider::start(session, night_light)
            .await
            .context("another glimpse-sunset already owns the night light")?;

        // The compositor is the one backend that must be there: without gamma control this process
        // has nothing to do, and saying so at start beats degrading for ever.
        let gamma = tokio::task::block_in_place(WaylandGamma::connect)
            .map_err(anyhow::Error::msg)
            .context("cannot take gamma control")?;

        let location_sender = location_runtime.sender();
        let night_sender = night_runtime.sender();
        let started = vec![
            spawn(location_runtime, ()),
            spawn(
                solar_runtime,
                SolarDependencies {
                    geolocation: location,
                },
            ),
            spawn(
                night_runtime,
                NightLightDependencies {
                    solar,
                    gamma: Box::new(gamma),
                },
            ),
        ];

        tracing::info!("night light service graph started");
        Ok(Self {
            location: location_sender,
            night: night_sender,
            started,
            cancel,
            provider: Some(provider),
        })
    }

    pub fn reconfigure(&self, document: &Config) {
        self.location
            .reconfigure(<Geolocation as Service>::Config::from(document));
        self.night.reconfigure(NightLightConfig::from(document));
    }

    /// The night light's `Service::stop` hands the outputs back while the compositor connection is
    /// still up. A `SIGKILL` cannot run it and the ramp then outlives the process — that is the
    /// protocol, documented in the crate README rather than guarded against.
    pub async fn shutdown(mut self) {
        tracing::info!("night light shutting down");
        if let Some(provider) = self.provider.take() {
            provider.shutdown().await;
        }
        self.cancel.cancel();
        for (service, task) in std::mem::take(&mut self.started).into_iter().rev() {
            if let Err(error) = task.await {
                tracing::error!(service, %error, "service task failed");
            }
        }
        tracing::info!("night light service graph stopped");
    }
}

impl Drop for SunsetServices {
    fn drop(&mut self) {
        if let Some(provider) = &self.provider {
            provider.cancel();
        }
        self.cancel.cancel();
    }
}

fn spawn<S: Service>(
    mut runtime: ServiceRuntime<S>,
    dependencies: S::Dependencies,
) -> (&'static str, JoinHandle<()>) {
    let task = tokio::spawn(async move {
        if let Err(error) = runtime.run(dependencies).await {
            tracing::error!(service = S::NAME, %error, "service stopped");
        }
    });
    (S::NAME, task)
}
