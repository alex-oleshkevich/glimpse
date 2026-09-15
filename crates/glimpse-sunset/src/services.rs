use anyhow::{Context as _, Result};
use glimpse_config::Config;
use glimpse_dbus::Buses;
use glimpse_services::{
    Geolocation, NightLight, NightLightDependencies, Running, Solar, SolarDependencies,
};

use crate::gamma::WaylandGamma;
use crate::provider;

pub struct SunsetServices {
    location_service: Running<Geolocation>,
    solar_service: Running<Solar>,
    night_service: Running<NightLight>,
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

        let (location_pending, location) = Running::<Geolocation>::build(document, buses.clone());
        let (solar_pending, solar) = Running::<Solar>::build(document, buses.clone());
        let (night_pending, night_light) = Running::<NightLight>::build(document, buses);

        let provider = provider::start(session, night_light)
            .await
            .context("another glimpse-sunset already owns the night light")?;

        // The compositor is the one backend that must be there: without gamma control this process
        // has nothing to do, and saying so at start beats degrading for ever.
        let gamma = tokio::task::block_in_place(WaylandGamma::connect)
            .map_err(anyhow::Error::msg)
            .context("cannot take gamma control")?;

        let location_service = location_pending.start(());
        let solar_service = solar_pending.start(SolarDependencies {
            geolocation: location,
        });
        let night_service = night_pending.start(NightLightDependencies {
            solar,
            gamma: Box::new(gamma),
        });

        tracing::info!("night light service graph started");
        Ok(Self {
            location_service,
            solar_service,
            night_service,
            provider: Some(provider),
        })
    }

    pub fn reconfigure(&self, document: &Config) {
        self.location_service.reconfigure(document);
        self.night_service.reconfigure(document);
    }

    /// The night light's `Service::stop` hands the outputs back while the compositor connection is
    /// still up. A `SIGKILL` cannot run it and the ramp then outlives the process — that is the
    /// protocol, documented in the crate README rather than guarded against.
    ///
    /// Stop order is the reverse of start order, so the night light hands the outputs back while
    /// everything it reads is still alive.
    pub async fn shutdown(mut self) {
        tracing::info!("night light shutting down");
        if let Some(provider) = self.provider.take() {
            provider.shutdown().await;
        }
        self.night_service.cancel();
        self.solar_service.cancel();
        self.location_service.cancel();
        self.night_service.stop().await;
        self.solar_service.stop().await;
        self.location_service.stop().await;
        tracing::info!("night light service graph stopped");
    }
}
