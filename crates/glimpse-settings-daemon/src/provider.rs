use glimpse_dbus::settings::{
    GLIMPSE_SETTINGS_BUS_NAME, GLIMPSE_SETTINGS_OBJECT_PATH, GeolocationSnapshot, Mode,
    ModeSnapshot, SolarPhase, SolarSnapshot,
};
use glimpse_dbus::{Exported, Snapshot};
use glimpse_services::{CommandError, DesktopModeHandle, GeolocationHandle, SolarHandle};

pub struct Provider {
    solar: SolarHandle,
    location: GeolocationHandle,
    mode: DesktopModeHandle,
}

impl Snapshot for Provider {
    async fn emit_snapshot_changed(
        &self,
        emitter: &zbus::object_server::SignalEmitter<'_>,
    ) -> zbus::Result<()> {
        self.solar_changed(emitter).await?;
        self.geolocation_changed(emitter).await?;
        self.mode_changed(emitter).await
    }
}

#[zbus::interface(name = "me.aresa.Glimpse.Settings1")]
impl Provider {
    #[zbus(property)]
    fn mode(&self) -> ModeSnapshot {
        let health = self.mode.health();
        let reason = health
            .borrow()
            .unavailable_reason()
            .map_or_else(String::new, |reason| glimpse_utils::clean(reason, 240));
        let status = self.mode.snapshot();
        (
            reason.is_empty(),
            status.policy.as_str().into(),
            status.requested.map_or(0, Mode::code),
            status.observed.map_or(0, Mode::code),
            status.temporary.map_or(0, Mode::code),
            reason,
        )
    }

    async fn set_temporary_mode(&self, mode: u32) -> zbus::fdo::Result<()> {
        let mode = Mode::decode(mode)
            .map_err(zbus::fdo::Error::InvalidArgs)?
            .ok_or_else(|| {
                zbus::fdo::Error::InvalidArgs("temporary mode must be light (1) or dark (2)".into())
            })?;
        self.mode.set_temporary(mode).await.map_err(command_error)
    }

    async fn resume(&self) -> zbus::fdo::Result<()> {
        self.mode.resume().await.map_err(command_error)
    }

    #[zbus(property)]
    fn geolocation(&self) -> GeolocationSnapshot {
        let reason = self
            .location
            .health()
            .borrow()
            .unavailable_reason()
            .map_or_else(String::new, |reason| glimpse_utils::clean(reason, 240));
        match self.location.snapshot().coordinates {
            Some(fix) => (true, fix.latitude, fix.longitude, reason),
            None => (false, 0.0, 0.0, reason),
        }
    }

    #[zbus(property)]
    fn solar(&self) -> SolarSnapshot {
        let health = self.solar.health();
        if let Some(reason) = health.borrow().unavailable_reason() {
            return (false, 0, 0, glimpse_utils::clean(reason, 240));
        }
        match self.solar.snapshot() {
            Some(status) => (
                true,
                match status.phase {
                    SolarPhase::Day => 1,
                    SolarPhase::Night => 2,
                },
                status.next_change.map_or(0, |time| time.timestamp_micros()),
                String::new(),
            ),
            None => (false, 0, 0, "no solar phase yet".to_owned()),
        }
    }
}

pub type Runtime = Exported<Provider>;

pub async fn start(
    connection: zbus::Connection,
    solar: SolarHandle,
    location: GeolocationHandle,
    mode: DesktopModeHandle,
) -> zbus::Result<Runtime> {
    let mut solar_state = solar.subscribe();
    let mut solar_health = solar.health();
    let mut location_state = location.subscribe();
    let mut location_health = location.health();
    let mut mode_state = mode.subscribe();
    let mut mode_health = mode.health();
    let source = connection.clone();
    let changes = async move {
        loop {
            tokio::select! {
                changed = solar_state.changed() => if changed.is_err() { return },
                changed = solar_health.changed() => if changed.is_err() { return },
                changed = location_state.changed() => if changed.is_err() { return },
                changed = location_health.changed() => if changed.is_err() { return },
                changed = mode_state.changed() => if changed.is_err() { return },
                changed = mode_health.changed() => if changed.is_err() { return },
            }
            let Ok(interface) = source
                .object_server()
                .interface::<_, Provider>(GLIMPSE_SETTINGS_OBJECT_PATH)
                .await
            else {
                return;
            };
            let provider = interface.get().await;
            if let Err(error) = provider
                .emit_snapshot_changed(interface.signal_emitter())
                .await
            {
                tracing::warn!(%error, "cannot publish settings changes");
            }
        }
    };
    Exported::start(
        connection,
        GLIMPSE_SETTINGS_BUS_NAME,
        GLIMPSE_SETTINGS_OBJECT_PATH,
        Provider {
            solar,
            location,
            mode,
        },
        changes,
    )
    .await
}

fn command_error(error: CommandError) -> zbus::fdo::Error {
    let reason = glimpse_utils::clean(&error.to_string(), 240);
    match error {
        CommandError::InvalidArgument(_) => zbus::fdo::Error::InvalidArgs(reason),
        CommandError::Unsupported(_) => zbus::fdo::Error::NotSupported(reason),
        _ => zbus::fdo::Error::Failed(reason),
    }
}
