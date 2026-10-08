use anyhow::{Context as _, Result};
use glimpse_config::Config;
use glimpse_dbus::Buses;
use glimpse_dbus::actions::{self, ActionsHandle};
use glimpse_dbus::night_light::GLIMPSE_NIGHT_LIGHT_OBJECT_PATH;
use glimpse_dbus::settings::SettingsProvider;
use glimpse_services::{NightLight, NightLightDependencies, Running};

use crate::gamma::WaylandGamma;
use crate::provider;

pub struct SunsetServices {
    solar: SettingsProvider,
    night_service: Running<NightLight>,
    /// `Option` only so `shutdown` can move it out of a type that also implements `Drop`.
    provider: Option<provider::Runtime>,
    actions: Option<ActionsHandle>,
}

impl SunsetServices {
    /// The order is the whole of this function. Building a `ServiceRuntime` allocates channels and
    /// nothing else, so the handles exist before anything has a side effect; the D-Bus object and
    /// name are taken next, which is where a second copy of this binary fails; only then is gamma
    /// control taken and the services actually started. A duplicate therefore never touches the
    /// outputs the running one is holding.
    pub async fn start(document: &Config, initial: Result<(), String>) -> Result<Self> {
        let buses = Buses::connect().await;
        let session = buses
            .session_bus()
            .cloned()
            .map_err(|reason| anyhow::anyhow!(reason.to_owned()))
            .context("the night light provider needs the session bus")?;

        let (night_pending, night_light) = Running::<NightLight>::build(document, buses);

        let actions = match actions::serve(
            &session,
            GLIMPSE_NIGHT_LIGHT_OBJECT_PATH,
            crate::cli::build::PKG_VERSION,
            initial,
        )
        .await
        {
            Ok(actions) => Some(actions),
            Err(error) => {
                tracing::warn!(%error, "cannot export night light actions");
                None
            }
        };

        let provider = provider::start(session.clone(), night_light)
            .await
            .context("another glimpse-sunset already owns the night light")?;

        // The compositor is the one backend that must be there: without gamma control this process
        // has nothing to do, and saying so at start beats degrading for ever.
        let gamma = tokio::task::block_in_place(WaylandGamma::connect)
            .map_err(anyhow::Error::msg)
            .context("cannot take gamma control")?;

        let solar = SettingsProvider::start(session);
        let night_service = night_pending.start(NightLightDependencies {
            solar: solar.subscribe(),
            gamma: Box::new(gamma),
        });

        tracing::info!("night light service graph started");
        Ok(Self {
            solar,
            night_service,
            provider: Some(provider),
            actions,
        })
    }

    pub fn reconfigure(&self, document: &Config) {
        self.night_service.reconfigure(document);
    }

    pub async fn report(&self, verdict: Result<(), String>) {
        if let Some(actions) = &self.actions
            && let Err(error) = actions.report(verdict).await
        {
            tracing::warn!(%error, "cannot report night light configuration status");
        }
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
        self.night_service.stop().await;
        self.solar.shutdown().await;
        tracing::info!("night light service graph stopped");
    }
}
