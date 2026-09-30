use std::path::PathBuf;

use anyhow::Result;
use glimpse_dbus::night_light::{GLIMPSE_NIGHT_LIGHT_BUS_NAME, NightLight1Proxy};
use glimpse_dbus::notifications::{GLIMPSE_NOTIFICATIONS_BUS_NAME, Notifications1Proxy};
use glimpse_dbus::weather::{GLIMPSE_WEATHER_BUS_NAME, Weather1Proxy};
use serde::Serialize;
use zbus::Connection;

use super::{emit, has_owner, proxy, safe, within};
use crate::errors::Exit;
use crate::render::{self, Section, Table, styled};

pub async fn doctor(config: Option<PathBuf>, json: bool) -> Result<()> {
    let session = Connection::session().await;
    let configuration = configuration(config);
    let providers = providers(&session).await;

    if json {
        return emit(&Diagnosis {
            configuration: &configuration,
            providers: &providers,
        });
    }

    configuration.section().print()?;
    render::print("")?;
    providers_section(&providers, &session).print()
}

#[derive(Serialize)]
struct Diagnosis<'a> {
    configuration: &'a Configuration,
    providers: &'a [Provider],
}

#[derive(Serialize)]
pub struct Configuration {
    pub files: Vec<PathBuf>,
    pub loads: bool,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct Provider {
    name: &'static str,
    pub running: bool,
    serving: bool,
    reason: String,
}

fn configuration(config: Option<PathBuf>) -> Configuration {
    let files = glimpse_config::resolved_files(config.as_deref());
    let loaded = glimpse_config::load(config.as_deref());
    Configuration {
        files: files.unwrap_or_default(),
        loads: loaded.is_ok(),
        error: loaded.err().map(|error| error.to_string()),
    }
}

impl Configuration {
    pub fn section(&self) -> Section {
        let section = Section::new("Configuration").with(
            Table::new()
                .with_empty("no configuration file, so every default applies")
                .with_rows(self.files.iter().map(|file| [file.display().to_string()]))
                .render(),
        );

        match &self.error {
            None => section.with_note("the stack loads"),
            Some(error) => section.with(styled::bad(error)).with_note(
                "no binary starts until this is fixed; they exit on it rather than falling back \
                 to defaults",
            ),
        }
    }
}

async fn providers(session: &Result<Connection, zbus::Error>) -> Vec<Provider> {
    let Ok(connection) = session else {
        return Vec::new();
    };
    let probes = tokio::join!(
        probe(
            connection,
            GLIMPSE_NIGHT_LIGHT_BUS_NAME,
            night_light(connection),
        ),
        probe(connection, GLIMPSE_WEATHER_BUS_NAME, weather(connection)),
        probe(
            connection,
            GLIMPSE_NOTIFICATIONS_BUS_NAME,
            notifications(connection),
        ),
    );
    vec![probes.0, probes.1, probes.2]
}

fn providers_section(providers: &[Provider], session: &Result<Connection, zbus::Error>) -> Section {
    if let Err(error) = session {
        return Section::new("Providers")
            .with(styled::bad(&error.to_string()))
            .with_note("no provider can be reached without the session bus");
    }

    let section = Section::new("Providers").with(
        Table::new()
            .with_headers(["PROVIDER", "STATE", "DETAIL"])
            .with_rows(providers.iter().map(Provider::row))
            .render(),
    );

    match providers.iter().filter(|p| !p.running).count() {
        0 => section,
        1 => section.with_note("1 provider is not running"),
        absent => section.with_note(&format!("{absent} providers are not running")),
    }
}

impl Provider {
    pub fn absent(name: &'static str, reason: String) -> Self {
        Self {
            name,
            running: false,
            serving: false,
            reason,
        }
    }

    pub fn answered(name: &'static str, serving: bool, reason: String) -> Self {
        Self {
            name,
            running: true,
            serving,
            reason,
        }
    }

    pub fn row(&self) -> [String; 3] {
        let state = match (self.running, self.serving) {
            (false, _) => styled::bad("not running"),
            (true, true) => styled::good("serving"),
            (true, false) => styled::warn("degraded"),
        };
        let detail = match self.reason.is_empty() {
            true => String::new(),
            false => styled::key(&self.reason),
        };
        [self.name.to_owned(), state, detail]
    }
}

async fn probe(
    connection: &Connection,
    name: &'static str,
    ask: impl Future<Output = Result<(bool, String)>>,
) -> Provider {
    match has_owner(connection, name).await {
        Ok(true) => {}
        Ok(false) => return Provider::absent(name, "nobody owns the name".to_owned()),
        Err(error) => return Provider::absent(name, error.to_string()),
    }
    match ask.await {
        Ok((serving, reason)) => Provider::answered(name, serving, safe(&reason)),
        Err(error) => {
            let detail = safe(&error.to_string());
            match crate::errors::exit(&error) {
                Exit::Unreachable => Provider::absent(name, detail),
                _ => Provider::answered(name, false, detail),
            }
        }
    }
}

async fn night_light(connection: &Connection) -> Result<(bool, String)> {
    let snapshot = within(proxy::<NightLight1Proxy>(connection).await?.snapshot()).await?;
    Ok((snapshot.serving, snapshot.reason))
}

async fn weather(connection: &Connection) -> Result<(bool, String)> {
    let (available, _stale, reason, ..) =
        within(proxy::<Weather1Proxy>(connection).await?.snapshot()).await?;
    Ok((available, reason))
}

async fn notifications(connection: &Connection) -> Result<(bool, String)> {
    let (_, _, serving, reason) =
        within(proxy::<Notifications1Proxy>(connection).await?.snapshot()).await?;
    Ok((serving, reason))
}
