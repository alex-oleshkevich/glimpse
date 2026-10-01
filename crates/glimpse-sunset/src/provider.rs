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
mod tests;
