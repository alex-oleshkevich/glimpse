mod notify;
mod request;
mod validate;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use request::{Flight, PortalRequest, finish};
use validate::{collect_copies, copy_image, file_path_from_uri, reason_text};
use zbus::Connection;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue};

const BUS_NAME: &str = "me.aresa.Glimpse.Wallpaper.Portal";
const OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
const TABLE: &str = "wallpaper";
const SUCCESS: u32 = 0;
const OTHER: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Background,
    Lockscreen,
}

fn target_of(options: &HashMap<String, OwnedValue>) -> Target {
    match option_string(options, "set-on").as_deref() {
        Some("lockscreen") => Target::Lockscreen,
        _ => Target::Background,
    }
}

fn option_string(options: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    options
        .get(key)
        .and_then(|value| String::try_from(value.clone()).ok())
}

fn outputs_kept(config: &glimpse_config::Config) -> usize {
    config
        .wallpaper
        .outputs
        .iter()
        .filter(|output| output.image.is_some() || output.image_dark.is_some())
        .count()
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Prev {
    image: Option<String>,
    image_dark: Option<String>,
}

fn prev_from(text: &str) -> Result<Prev, toml_edit::TomlError> {
    let document = text.parse::<toml_edit::DocumentMut>()?;
    let table = document.get(TABLE).and_then(toml_edit::Item::as_table_like);
    let read = |key: &str| {
        table
            .and_then(|table| table.get(key))
            .and_then(toml_edit::Item::as_value)
            .and_then(toml_edit::Value::as_str)
            .map(str::to_owned)
    };
    Ok(Prev {
        image: read("image"),
        image_dark: read("image-dark"),
    })
}

fn read_prev(path: &Path) -> Result<Prev, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => prev_from(&text).map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Prev::default()),
        Err(error) => Err(error.to_string()),
    }
}

fn restore_values(prev: &Prev) -> (toml_edit::Table, Vec<&'static str>) {
    let mut values = toml_edit::Table::new();
    let mut remove = Vec::new();
    match &prev.image {
        Some(image) => {
            values.insert("image", toml_edit::value(image.as_str()));
        }
        None => remove.push("image"),
    }
    match &prev.image_dark {
        Some(image) => {
            values.insert("image-dark", toml_edit::value(image.as_str()));
        }
        None => remove.push("image-dark"),
    }
    (values, remove)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub struct Settings {
    config: PathBuf,
    files: Vec<PathBuf>,
    copies: PathBuf,
}

fn settings() -> Option<Settings> {
    Some(Settings {
        config: glimpse_config::writable_file("config")?,
        files: glimpse_config::config_files(glimpse_config::Documents::Config).ok()?,
        copies: glimpse_config::state_dir()?.join("wallpaper"),
    })
}

struct Inner {
    settings: Settings,
    flight: Mutex<Option<Arc<Flight>>>,
    pending: Mutex<Option<u32>>,
    epoch: AtomicU64,
}

impl Inner {
    fn reserve(&self) -> Option<Arc<Flight>> {
        let mut slot = lock(&self.flight);
        if slot.is_some() {
            return None;
        }
        let flight = Arc::new(Flight::new());
        *slot = Some(Arc::clone(&flight));
        Some(flight)
    }

    fn release(&self, flight: &Arc<Flight>) {
        let mut slot = lock(&self.flight);
        if slot
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, flight))
        {
            *slot = None;
        }
    }

    fn undo(&self, epoch: u64, prev: &Prev) {
        if self.epoch.load(Ordering::SeqCst) != epoch {
            return;
        }
        let (values, remove) = restore_values(prev);
        if let Err(error) =
            glimpse_config::write_table(&self.settings.config, TABLE, &values, &remove)
        {
            tracing::warn!(%error, "cannot restore the previous wallpaper");
            return;
        }
        *lock(&self.pending) = None;
        let keep: Vec<PathBuf> = [prev.image.as_ref(), prev.image_dark.as_ref()]
            .into_iter()
            .flatten()
            .map(PathBuf::from)
            .filter(|path| path.starts_with(&self.settings.copies))
            .collect();
        collect_copies(&self.settings.copies, &keep);
    }
}

pub fn run() {
    let Some(settings) = settings() else {
        tracing::warn!("no state or config directory; the wallpaper portal is off");
        return;
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::warn!(%error, "cannot start the wallpaper portal");
            return;
        }
    };
    let served = runtime.block_on(async move {
        let connection = zbus::Connection::session().await?;
        serve_at(&connection, settings).await?;
        glimpse_dbus::own_name(&connection, BUS_NAME).await?;
        tracing::info!(bus_name = BUS_NAME, "serving the wallpaper portal");
        std::future::pending::<anyhow::Result<()>>().await
    });
    if let Err(error) = served {
        tracing::warn!(%error, "the wallpaper portal is off");
    }
}

async fn serve_at(connection: &Connection, settings: Settings) -> anyhow::Result<()> {
    let portal = WallpaperPortal {
        inner: Arc::new(Inner {
            settings,
            flight: Mutex::new(None),
            pending: Mutex::new(None),
            epoch: AtomicU64::new(0),
        }),
    };
    if !connection.object_server().at(OBJECT_PATH, portal).await? {
        anyhow::bail!("wallpaper portal object path is already taken");
    }
    Ok(())
}

struct WallpaperPortal {
    inner: Arc<Inner>,
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Wallpaper")]
impl WallpaperPortal {
    #[zbus(name = "SetWallpaperURI")]
    async fn set_wallpaper_uri(
        &self,
        handle: ObjectPath<'_>,
        _app_id: String,
        _parent_window: String,
        uri: String,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] connection: &Connection,
    ) -> zbus::fdo::Result<u32> {
        let Some(flight) = self.inner.reserve() else {
            tracing::debug!("wallpaper portal request refused because another is open");
            return Ok(OTHER);
        };
        let owned: OwnedObjectPath = handle.to_owned().into();
        let request = PortalRequest {
            flight: Arc::clone(&flight),
            handle: owned.clone(),
        };
        match connection.object_server().at(owned.clone(), request).await {
            Ok(true) => {}
            taken => {
                tracing::warn!(?taken, handle = %owned, "wallpaper portal request path is already taken");
                self.inner.release(&flight);
                return finish(&flight, OTHER).await;
            }
        }
        let reply = match self.work(connection, &flight, &uri, &options).await {
            Ok(()) => SUCCESS,
            Err(body) => {
                if let Some(body) = body {
                    notify::plain(connection, notify::not_changed_summary(), body).await;
                }
                OTHER
            }
        };
        let _ = connection
            .object_server()
            .remove::<PortalRequest, _>(owned)
            .await;
        self.inner.release(&flight);
        finish(&flight, reply).await
    }
}

impl WallpaperPortal {
    async fn work(
        &self,
        connection: &Connection,
        flight: &Flight,
        uri: &str,
        options: &HashMap<String, OwnedValue>,
    ) -> Result<(), Option<String>> {
        if target_of(options) == Target::Lockscreen {
            return Err(Some(notify::lockscreen_body()));
        }
        let inner = Arc::clone(&self.inner);
        let (prev, kept) = tokio::task::spawn_blocking(move || {
            let settings = &inner.settings;
            let prev = read_prev(&settings.config).map_err(|error| {
                tracing::warn!(%error, "cannot read the wallpaper configuration");
                Some(notify::failed_body())
            })?;
            let config = glimpse_config::load_files(&settings.files).map_err(|error| {
                tracing::warn!(%error, "cannot load the wallpaper configuration");
                Some(notify::failed_body())
            })?;
            Ok((prev, outputs_kept(&config)))
        })
        .await
        .unwrap_or_else(|_| Err(Some(notify::failed_body())))?;
        let source = file_path_from_uri(uri).map_err(|reject| Some(reason_text(reject)))?;
        let copy = copy_image(&source, &self.inner.settings.copies)
            .await
            .map_err(|reject| Some(reason_text(reject)))?;
        if flight.is_closed() {
            let _ = tokio::fs::remove_file(&copy).await;
            return Err(None);
        }
        let inner = Arc::clone(&self.inner);
        let prev = tokio::task::spawn_blocking(move || {
            let settings = &inner.settings;
            let copy_text = copy.to_string_lossy().into_owned();
            let mut values = toml_edit::Table::new();
            values.insert("image", toml_edit::value(copy_text.as_str()));
            values.insert("image-dark", toml_edit::value(copy_text.as_str()));
            if let Err(error) = glimpse_config::write_table(&settings.config, TABLE, &values, &[]) {
                tracing::warn!(%error, "cannot write the wallpaper configuration");
                let _ = std::fs::remove_file(&copy);
                return Err(Some(notify::failed_body()));
            }
            let mut keep = vec![copy];
            for previous in [prev.image.as_ref(), prev.image_dark.as_ref()]
                .into_iter()
                .flatten()
            {
                let path = PathBuf::from(previous);
                if path.starts_with(&settings.copies) {
                    keep.push(path);
                }
            }
            collect_copies(&settings.copies, &keep);
            Ok(prev)
        })
        .await
        .unwrap_or_else(|_| Err(Some(notify::failed_body())))?;
        self.announce(connection, kept, prev);
        Ok(())
    }

    fn announce(&self, connection: &Connection, kept: usize, prev: Prev) {
        let epoch = self.inner.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        let replaces_id = lock(&self.inner.pending).unwrap_or(0);
        let connection = connection.clone();
        let inner = Arc::clone(&self.inner);
        let posted = Arc::clone(&self.inner);
        tokio::spawn(async move {
            notify::changed(
                &connection,
                replaces_id,
                notify::changed_body(kept),
                move |id| {
                    *lock(&posted.pending) = Some(id);
                },
                move || {
                    tokio::task::spawn_blocking(move || inner.undo(epoch, &prev));
                },
            )
            .await;
        });
    }
}

#[cfg(test)]
mod tests;
