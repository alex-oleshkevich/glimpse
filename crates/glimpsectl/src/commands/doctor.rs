use std::path::PathBuf;

use anyhow::Result;
use glimpse_dbus::night_light::{GLIMPSE_NIGHT_LIGHT_BUS_NAME, NightLight1Proxy};
use glimpse_dbus::notifications::{GLIMPSE_NOTIFICATIONS_BUS_NAME, Notifications1Proxy};
use glimpse_dbus::weather::{GLIMPSE_WEATHER_BUS_NAME, Weather1Proxy};
use serde::Serialize;
use zbus::Connection;

use super::{emit, has_owner, proxy, safe};
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
struct Configuration {
    files: Vec<PathBuf>,
    loads: bool,
    error: Option<String>,
}

#[derive(Serialize)]
struct Provider {
    name: &'static str,
    running: bool,
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
    fn section(&self) -> Section {
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
    vec![
        probe(
            connection,
            GLIMPSE_NIGHT_LIGHT_BUS_NAME,
            night_light(connection),
        )
        .await,
        probe(connection, GLIMPSE_WEATHER_BUS_NAME, weather(connection)).await,
        probe(
            connection,
            GLIMPSE_NOTIFICATIONS_BUS_NAME,
            notifications(connection),
        )
        .await,
    ]
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
    fn absent(name: &'static str, reason: String) -> Self {
        Self {
            name,
            running: false,
            serving: false,
            reason,
        }
    }

    fn answered(name: &'static str, serving: bool, reason: String) -> Self {
        Self {
            name,
            running: true,
            serving,
            reason,
        }
    }

    fn row(&self) -> [String; 3] {
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
        Err(error) => Provider::absent(name, error.to_string()),
    }
}

async fn night_light(connection: &Connection) -> Result<(bool, String)> {
    let snapshot = proxy::<NightLight1Proxy>(connection)
        .await?
        .snapshot()
        .await?;
    Ok((snapshot.serving, snapshot.reason))
}

async fn weather(connection: &Connection) -> Result<(bool, String)> {
    let (available, _stale, reason, ..) =
        proxy::<Weather1Proxy>(connection).await?.snapshot().await?;
    Ok((available, reason))
}

async fn notifications(connection: &Connection) -> Result<(bool, String)> {
    let (_, _, serving, reason) = proxy::<Notifications1Proxy>(connection)
        .await?
        .snapshot()
        .await?;
    Ok((serving, reason))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_provider_that_answers_but_is_not_serving_is_not_reported_healthy() {
        let degraded = Provider::answered(
            "x",
            false,
            "another gamma client holds the outputs".to_owned(),
        );
        assert!(degraded.running, "it answered, so it is running");
        let [_, state, detail] = degraded.row();
        assert!(state.contains("degraded"));
        assert!(detail.contains("another gamma client"));
    }

    #[test]
    fn a_serving_provider_has_no_detail_to_add() {
        let [_, state, detail] = Provider::answered("x", true, String::new()).row();
        assert!(state.contains("serving"));
        assert!(detail.is_empty());
    }

    #[test]
    fn an_absent_provider_is_not_running_and_says_why() {
        let absent = Provider::absent("x", "nobody owns the name".to_owned());
        assert!(!absent.running);
        let [_, state, detail] = absent.row();
        assert!(state.contains("not running"));
        assert!(detail.contains("nobody owns the name"));
    }

    #[test]
    fn a_configuration_that_does_not_load_says_binaries_will_not_start() {
        let broken = Configuration {
            files: vec![PathBuf::from("/etc/glimpse/config.toml")],
            loads: false,
            error: Some("expected a table".to_owned()),
        };
        let rendered = broken.section().render();
        assert!(rendered.contains("expected a table"));
        assert!(rendered.contains("no binary starts"));
        assert!(!rendered.contains("falls back to defaults\n"));
    }
}
