use anyhow::{Context as _, Result};
use glimpse_config::Config;
use glimpse_dbus::Buses;
use glimpse_dbus::actions::{self, ActionsHandle};
use glimpse_dbus::settings::GLIMPSE_SETTINGS_OBJECT_PATH;
use glimpse_services::{
    DesktopMode, Geolocation, GeolocationDependencies, Running, Solar, SolarDependencies,
};

use crate::{mode, provider};

pub struct SettingsServices {
    location: Running<Geolocation>,
    solar: Running<Solar>,
    mode: Running<DesktopMode>,
    backend: mode::Backend,
    provider: provider::Runtime,
    actions: ActionsHandle,
}

impl SettingsServices {
    pub async fn start(document: &Config, initial: Result<(), String>) -> Result<Self> {
        let location = GeolocationDependencies {
            cache: glimpse_config::state_dir().map(|dir| dir.join("location.json")),
        };
        Self::start_with_buses(document, Buses::connect().await, location, initial).await
    }

    async fn start_with_buses(
        document: &Config,
        buses: Buses,
        location_dependencies: GeolocationDependencies,
        initial: Result<(), String>,
    ) -> Result<Self> {
        let session = buses
            .session_bus()
            .cloned()
            .map_err(|reason| anyhow::anyhow!(reason.to_owned()))
            .context("the settings provider needs the session bus")?;
        let actions = actions::serve(
            &session,
            GLIMPSE_SETTINGS_OBJECT_PATH,
            env!("CARGO_PKG_VERSION"),
            initial,
        )
        .await
        .context("cannot export settings actions")?;
        let (location_pending, location) = Running::<Geolocation>::build(document, buses.clone());
        let (solar_pending, solar) = Running::<Solar>::build(document, buses.clone());
        let (mode_pending, mode) = Running::<DesktopMode>::build(document, buses);
        let provider = provider::start(session, solar.clone(), location.clone(), mode)
            .await
            .context("cannot acquire the settings D-Bus name")?;
        let (backend, dependencies) =
            mode::Backend::start(solar.subscribe()).context("cannot start desktop mode backend")?;
        let mode = mode_pending.start(dependencies);
        let location_service = location_pending.start(location_dependencies);
        let solar = solar_pending.start(SolarDependencies {
            geolocation: location,
        });
        Ok(Self {
            location: location_service,
            solar,
            mode,
            backend,
            provider,
            actions,
        })
    }

    pub fn reconfigure(&self, document: &Config) {
        self.location.reconfigure(document);
        self.mode.reconfigure(document);
    }

    pub async fn report(&self, verdict: Result<(), String>) {
        if let Err(error) = self.actions.report(verdict).await {
            tracing::warn!(%error, "cannot report settings configuration status");
        }
    }

    pub async fn shutdown(mut self) {
        self.provider.shutdown().await;
        self.mode.stop().await;
        self.backend.shutdown().await;
        self.solar.cancel();
        self.location.cancel();
        self.solar.stop().await;
        self.location.stop().await;
    }
}

#[cfg(test)]
mod tests;
