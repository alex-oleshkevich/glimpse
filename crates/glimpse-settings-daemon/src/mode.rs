use futures_util::future::BoxFuture;
use gio::prelude::*;
use glimpse_dbus::settings::{Mode, SolarStatus};
use glimpse_services::{ModeDependencies, ModeWriter};
use tokio::sync::{mpsc, oneshot, watch};

const SCHEMA: &str = "org.gnome.desktop.interface";
const KEY: &str = "color-scheme";

struct Write {
    mode: Option<Mode>,
    reply: oneshot::Sender<Result<(), String>>,
}

struct Writer(mpsc::Sender<Write>);

impl ModeWriter for Writer {
    fn write(&self, mode: Option<Mode>) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            let (reply, answer) = oneshot::channel();
            self.0
                .send(Write { mode, reply })
                .await
                .map_err(|_| "desktop mode backend stopped".to_owned())?;
            answer
                .await
                .map_err(|_| "desktop mode backend stopped".to_owned())?
        })
    }
}

pub struct Backend {
    thread: std::thread::JoinHandle<()>,
}

impl Backend {
    pub fn start(
        solar: watch::Receiver<Option<SolarStatus>>,
    ) -> std::io::Result<(Self, ModeDependencies)> {
        let (requests, mut received) = mpsc::channel::<Write>(16);
        let (updates, observed) = watch::channel(Err("desktop mode backend starting".into()));
        let thread = std::thread::Builder::new()
            .name("desktop-mode".into())
            .spawn(move || {
                let context = glib::MainContext::new();
                if let Err(error) = context.with_thread_default(|| {
                    let settings = settings();
                    match &settings {
                        Ok(settings) => {
                            let changes = updates.clone();
                            settings.connect_changed(Some(KEY), move |settings, _| {
                                let _ = changes.send_replace(read(settings));
                            });
                            let _ = updates.send_replace(read(settings));
                        }
                        Err(reason) => {
                            let _ = updates.send_replace(Err(reason.clone()));
                        }
                    }
                    context.block_on(async move {
                        while let Some(write) = received.recv().await {
                            let result = match &settings {
                                Err(reason) => Err(reason.clone()),
                                Ok(settings) if !settings.is_writable(KEY) => {
                                    Err("desktop color-scheme is not writable".into())
                                }
                                Ok(settings) => {
                                    let result = settings
                                        .set_string(KEY, value(write.mode))
                                        .map_err(|error| error.to_string());
                                    gio::Settings::sync();
                                    result.and_then(|()| {
                                        (read(settings)? == write.mode).then_some(()).ok_or_else(
                                            || "desktop color-scheme write was not applied".into(),
                                        )
                                    })
                                }
                            };
                            let _ = write.reply.send(result);
                        }
                        gio::Settings::sync();
                    });
                }) {
                    tracing::error!(%error, "cannot run the desktop mode backend");
                }
            })?;
        Ok((
            Self { thread },
            ModeDependencies {
                solar,
                writer: Box::new(Writer(requests)),
                observed,
            },
        ))
    }

    pub async fn shutdown(self) {
        if !matches!(
            tokio::task::spawn_blocking(move || self.thread.join()).await,
            Ok(Ok(()))
        ) {
            tracing::warn!("desktop mode backend failed to shut down");
        }
    }
}

fn settings() -> Result<gio::Settings, String> {
    let schema = gio::SettingsSchemaSource::default()
        .and_then(|source| source.lookup(SCHEMA, true))
        .ok_or_else(|| "desktop color-scheme schema unavailable".to_owned())?;
    if !schema.has_key(KEY) {
        return Err("desktop color-scheme key unavailable".into());
    }
    let key = schema.key(KEY);
    for mode in [None, Some(Mode::Light), Some(Mode::Dark)] {
        if !key.range_check(&value(mode).to_variant()) {
            return Err("desktop color-scheme does not support light/dark preferences".into());
        }
    }
    Ok(gio::Settings::new_full(
        &schema,
        None::<&gio::SettingsBackend>,
        None,
    ))
}

fn value(mode: Option<Mode>) -> &'static str {
    match mode {
        None => "default",
        Some(Mode::Light) => "prefer-light",
        Some(Mode::Dark) => "prefer-dark",
    }
}

fn read(settings: &gio::Settings) -> Result<Option<Mode>, String> {
    match settings.string(KEY).as_str() {
        "default" => Ok(None),
        "prefer-light" => Ok(Some(Mode::Light)),
        "prefer-dark" => Ok(Some(Mode::Dark)),
        _ => Err("invalid desktop color-scheme preference".into()),
    }
}
