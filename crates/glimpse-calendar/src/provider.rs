use chrono::DateTime;
use glimpse_dbus::calendar::{
    CalendarRange, CalendarSnapshot, GLIMPSE_CALENDAR_BUS_NAME, GLIMPSE_CALENDAR_OBJECT_PATH,
    encode_range, encode_snapshot,
};
use glimpse_dbus::{Exported, Snapshot};
use glimpse_services::{CalendarHandle, CommandError};
use zbus::{Connection, DBusError};

#[derive(Debug, DBusError)]
#[zbus(prefix = "me.aresa.Glimpse.Calendar1.Error", impl_display = true)]
pub enum Error {
    InvalidRange(String),
    Unavailable(String),
    #[zbus(error)]
    ZBus(zbus::Error),
}

pub(crate) struct Provider {
    calendar: CalendarHandle,
}

impl Snapshot for Provider {
    async fn emit_snapshot_changed(
        &self,
        emitter: &zbus::object_server::SignalEmitter<'_>,
    ) -> zbus::Result<()> {
        self.snapshot_changed(emitter).await
    }
}

#[zbus::interface(name = "me.aresa.Glimpse.Calendar1")]
impl Provider {
    #[zbus(property)]
    fn snapshot(&self) -> CalendarSnapshot {
        let health = self.calendar.health();
        encode_snapshot(
            &self.calendar.snapshot(),
            health.borrow().unavailable_reason(),
        )
    }

    async fn events(&self, from_us: i64, to_us: i64) -> Result<CalendarRange, Error> {
        let from = DateTime::from_timestamp_micros(from_us)
            .ok_or_else(|| Error::InvalidRange("from timestamp is out of range".to_owned()))?;
        let to = DateTime::from_timestamp_micros(to_us)
            .ok_or_else(|| Error::InvalidRange("to timestamp is out of range".to_owned()))?;
        self.calendar
            .events(from, to)
            .await
            .map(|events| encode_range(&events))
            .map_err(Error::from)
    }

    async fn refresh(&self) -> Result<(), Error> {
        self.calendar.refresh().await.map_err(Error::from)
    }
}

impl From<CommandError> for Error {
    fn from(error: CommandError) -> Self {
        match error {
            CommandError::InvalidArgument(reason) => Self::InvalidRange(reason),
            error => Self::Unavailable(error.to_string()),
        }
    }
}

pub(crate) type Runtime = Exported<Provider>;

pub(crate) async fn start(
    connection: Connection,
    calendar: CalendarHandle,
) -> zbus::Result<Runtime> {
    let mut state = calendar.subscribe();
    state
        .wait_for(|state| state.revision != 0)
        .await
        .map_err(|error| zbus::Error::Failure(error.to_string()))?;
    Exported::serve(
        connection,
        GLIMPSE_CALENDAR_BUS_NAME,
        GLIMPSE_CALENDAR_OBJECT_PATH,
        Provider {
            calendar: calendar.clone(),
        },
        state,
        calendar.health(),
    )
    .await
}

#[cfg(test)]
mod tests;
