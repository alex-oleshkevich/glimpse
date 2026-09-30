use glimpse_dbus::Exported;
use glimpse_dbus::clipboard::{
    ClipboardItem, GLIMPSE_CLIPBOARD_BUS_NAME, GLIMPSE_CLIPBOARD_OBJECT_PATH,
};
use glimpse_services::{ClipboardEntry, ClipboardHandle, ClipboardKind, CommandError};
use tokio::task::JoinHandle;
use zbus::{Connection, DBusError};

#[derive(Debug, DBusError)]
#[zbus(prefix = "me.aresa.Glimpse.Clipboard1.Error", impl_display = true)]
pub enum Error {
    NoSuchEntry(String),
    Unavailable(String),
    #[zbus(error)]
    ZBus(zbus::Error),
}

impl From<CommandError> for Error {
    fn from(error: CommandError) -> Self {
        match error {
            CommandError::InvalidArgument(reason) => Self::NoSuchEntry(reason),
            error => Self::Unavailable(error.to_string()),
        }
    }
}

pub(crate) struct Provider {
    clipboard: ClipboardHandle,
}

#[zbus::interface(name = "me.aresa.Glimpse.Clipboard1")]
impl Provider {
    fn list(&self) -> Vec<ClipboardItem> {
        self.clipboard.snapshot().entries.iter().map(item).collect()
    }

    fn get(&self, id: u64) -> Result<(String, Vec<u8>), Error> {
        self.clipboard
            .snapshot()
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| (entry.mime.clone(), entry.data.to_vec()))
            .ok_or_else(|| Error::NoSuchEntry("there is no such entry".to_owned()))
    }

    async fn copy(&self, id: u64) -> Result<(), Error> {
        Ok(self.clipboard.restore(id).await?)
    }

    async fn pin(&self, id: u64, pinned: bool) -> Result<(), Error> {
        Ok(self.clipboard.pin(id, pinned).await?)
    }

    async fn forget(&self, id: u64) -> Result<(), Error> {
        Ok(self.clipboard.remove(id).await?)
    }

    async fn clear(&self) -> Result<(), Error> {
        Ok(self.clipboard.clear_history().await?)
    }
}

fn item(entry: &ClipboardEntry) -> ClipboardItem {
    ClipboardItem {
        id: entry.id,
        kind: match entry.kind {
            ClipboardKind::Text => "text",
            ClipboardKind::Image => "image",
        }
        .to_owned(),
        mime: entry.mime.clone(),
        pinned: entry.pinned,
        copied_at_secs: entry.at.timestamp(),
        size: entry.data.len() as u64,
        preview: entry.preview.clone(),
    }
}

pub(crate) struct Served(JoinHandle<Option<Exported<Provider>>>);

impl Served {
    pub(crate) fn start(connection: Connection, clipboard: ClipboardHandle) -> Self {
        Self(tokio::spawn(async move {
            Exported::start(
                connection,
                GLIMPSE_CLIPBOARD_BUS_NAME,
                GLIMPSE_CLIPBOARD_OBJECT_PATH,
                Provider { clipboard },
                std::future::pending::<()>(),
            )
            .await
            .inspect_err(|error| {
                tracing::warn!(%error, "the clipboard is not served on the session bus");
            })
            .ok()
        }))
    }

    pub(crate) async fn shutdown(self) {
        if let Ok(Some(exported)) = self.0.await {
            exported.shutdown().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use glimpse_dbus::Buses;
    use glimpse_services::{Clipboard, ClipboardConfig, ServiceRuntime};
    use tokio_util::sync::CancellationToken;

    use super::*;

    #[test]
    fn an_id_the_history_does_not_hold_is_no_such_entry() {
        let (_runtime, clipboard) = ServiceRuntime::<Clipboard>::new(
            ClipboardConfig::from(&glimpse_config::Config::default()),
            Buses::unavailable("no bus in tests"),
            CancellationToken::new(),
        );
        let provider = Provider { clipboard };

        assert!(provider.list().is_empty());
        assert!(matches!(provider.get(42), Err(Error::NoSuchEntry(_))));
    }

    #[test]
    fn a_refused_id_maps_to_no_such_entry_and_anything_else_to_unavailable() {
        assert!(matches!(
            Error::from(CommandError::InvalidArgument("gone".to_owned())),
            Error::NoSuchEntry(_)
        ));
        assert!(matches!(
            Error::from(CommandError::Unavailable("stopped".to_owned())),
            Error::Unavailable(_)
        ));
    }
}
