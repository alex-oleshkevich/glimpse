use std::path::PathBuf;

use anyhow::Result;
use glimpse_compositors::{Compositor, detect_compositor};
use glimpse_dbus::calendar::{Calendar1Proxy, GLIMPSE_CALENDAR_BUS_NAME};
use glimpse_dbus::idle::{GLIMPSE_IDLE_BUS_NAME, HealthKind, Idle1Proxy};
use glimpse_dbus::night_light::{GLIMPSE_NIGHT_LIGHT_BUS_NAME, NightLight1Proxy};
use glimpse_dbus::notifications::{GLIMPSE_NOTIFICATIONS_BUS_NAME, Notifications1Proxy};
use glimpse_dbus::settings::{
    GLIMPSE_SETTINGS_BUS_NAME, Settings1Proxy, decode_geolocation, decode_mode, decode_solar,
};
use glimpse_dbus::weather::{GLIMPSE_WEATHER_BUS_NAME, Weather1Proxy};
use serde::Serialize;
use zbus::Connection;

use super::{emit, safe, within};
use crate::errors::Exit;
use crate::render::{self, Section, Table, styled};

pub async fn doctor(json: bool) -> Result<()> {
    let session = within(Connection::session()).await;
    let configuration = configuration();
    let providers = providers(&session).await;
    let session_lock = session_lock(&detect_compositor())
        .await
        .filter(|lock| !lock.enabled);
    let components = match &session {
        Ok(connection) => super::status::rows(connection).await,
        Err(error) => Err(anyhow::anyhow!(error.to_string())),
    };
    let failed = !configuration.loads
        || !configuration.warnings.is_empty()
        || providers
            .iter()
            .any(|provider| !provider.running || !provider.serving)
        || session_lock.as_ref().is_some_and(|lock| !lock.enabled)
        || components.is_err();
    let components = components.unwrap_or_else(|_| super::status::absent_rows());

    if json {
        emit(&Diagnosis {
            configuration: &configuration,
            providers: &providers,
            components: &components,
            session_lock: session_lock.as_ref(),
        })?;
    } else {
        configuration.section().print()?;
        render::print("")?;
        providers_section(&providers, &session).print()?;
        render::print("")?;
        super::status::print(&components, false)?;
        if let Some(lock) = &session_lock {
            render::print("")?;
            lock.section().print()?;
        }
    }
    anyhow::ensure!(!failed, "diagnostic checks failed");
    Ok(())
}

#[derive(Serialize)]
struct Diagnosis<'a> {
    configuration: &'a Configuration,
    providers: &'a [Provider],
    components: &'a [super::status::ComponentRow],
    session_lock: Option<&'a SessionLock>,
}

const RESTORE_OPTION: &str = "misc:allow_session_lock_restore";
const RESTORE_FIX: &str = "misc { allow_session_lock_restore = true }";

#[derive(Serialize)]
pub struct SessionLock {
    pub option: &'static str,
    pub enabled: bool,
    pub fix: &'static str,
}

pub async fn session_lock(compositor: &Compositor) -> Option<SessionLock> {
    let enabled = match within(compositor.allows_session_lock_restore()).await {
        Ok(enabled) => enabled,
        Err(error) => {
            tracing::warn!(%error, "could not read {RESTORE_OPTION}");
            return None;
        }
    };
    Some(SessionLock {
        option: RESTORE_OPTION,
        enabled,
        fix: RESTORE_FIX,
    })
}

impl SessionLock {
    pub fn section(&self) -> Section {
        let section = Section::new("Session lock");
        match self.enabled {
            true => section.with_note(&format!("{} is on", self.option)),
            false => section
                .with(styled::bad(&format!(
                    "{} is off, so Hyprland refuses a restarted glimpse-lock",
                    self.option
                )))
                .with_note(&format!("set {}", self.fix)),
        }
    }
}

#[derive(Serialize)]
pub struct Configuration {
    pub files: Vec<PathBuf>,
    pub loads: bool,
    pub panels_only: bool,
    pub error: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
pub struct Provider {
    name: &'static str,
    pub running: bool,
    serving: bool,
    reason: String,
}

fn configuration() -> Configuration {
    let files = glimpse_config::config_files(glimpse_config::Documents::ConfigAndPanel);
    let loaded = glimpse_config::load(glimpse_config::Documents::ConfigAndPanel);
    Configuration {
        files: files.unwrap_or_default(),
        loads: loaded.is_ok(),
        panels_only: loaded.is_err()
            && glimpse_config::load(glimpse_config::Documents::Config).is_ok(),
        warnings: loaded
            .as_ref()
            .map(|config| {
                config
                    .validate()
                    .into_iter()
                    .map(|problem| problem.to_string())
                    .collect()
            })
            .unwrap_or_default(),
        error: loaded.err().map(|error| error.to_string()),
    }
}

impl Configuration {
    pub fn section(&self) -> Section {
        let section = Section::new("Configuration").with(
            Table::default()
                .with_empty("no configuration file, so every default applies")
                .with_rows(self.files.iter().map(|file| [file.display().to_string()]))
                .render(),
        );

        let section = self.warnings.iter().fold(section, |section, warning| {
            section.with(styled::warn(warning))
        });

        match &self.error {
            None => section.with_note("the stack loads"),
            Some(error) => section.with(styled::bad(error)).with_note(if self.panels_only {
                "the panel uses its default panels until this is fixed; the other binaries are unaffected"
            } else {
                "binaries start on the default configuration until this is fixed"
            }),
        }
    }
}

async fn providers(session: &Result<Connection>) -> Vec<Provider> {
    let Ok(connection) = session else {
        return [
            GLIMPSE_NIGHT_LIGHT_BUS_NAME,
            GLIMPSE_WEATHER_BUS_NAME,
            GLIMPSE_CALENDAR_BUS_NAME,
            GLIMPSE_NOTIFICATIONS_BUS_NAME,
            GLIMPSE_SETTINGS_BUS_NAME,
            GLIMPSE_IDLE_BUS_NAME,
        ]
        .into_iter()
        .map(|name| Provider::absent(name, "cannot reach the session bus".into()))
        .collect();
    };
    let probes = tokio::join!(
        probe(connection, GLIMPSE_NIGHT_LIGHT_BUS_NAME, |owner| {
            night_light(connection, owner)
        },),
        probe(connection, GLIMPSE_WEATHER_BUS_NAME, |owner| weather(
            connection, owner
        )),
        probe(connection, GLIMPSE_CALENDAR_BUS_NAME, |owner| calendar(
            connection, owner
        )),
        probe(connection, GLIMPSE_NOTIFICATIONS_BUS_NAME, |owner| {
            notifications(connection, owner)
        },),
        probe(connection, GLIMPSE_SETTINGS_BUS_NAME, |owner| settings(
            connection, owner
        )),
        probe(connection, GLIMPSE_IDLE_BUS_NAME, |owner| idle(
            connection, owner
        )),
    );
    vec![probes.0, probes.1, probes.2, probes.3, probes.4, probes.5]
}

fn providers_section(providers: &[Provider], session: &Result<Connection>) -> Section {
    if let Err(error) = session {
        return Section::new("Providers")
            .with(styled::bad(&error.to_string()))
            .with_note("no provider can be reached without the session bus");
    }

    let section = Section::new("Providers").with(
        Table::default()
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

async fn probe<F, R>(connection: &Connection, name: &'static str, ask: F) -> Provider
where
    F: FnOnce(zbus::names::OwnedUniqueName) -> R,
    R: Future<Output = Result<(bool, String)>>,
{
    let owner = async {
        let dbus = within(zbus::fdo::DBusProxy::new(connection)).await?;
        within(dbus.get_name_owner(name.try_into()?)).await
    }
    .await;
    let owner = match owner {
        Ok(owner) => owner,
        Err(error) => return Provider::absent(name, error.to_string()),
    };
    match ask(owner).await {
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

async fn owned_proxy<'a, P>(
    connection: &'a Connection,
    owner: zbus::names::OwnedUniqueName,
) -> Result<P>
where
    P: From<zbus::Proxy<'a>> + zbus::proxy::Defaults,
{
    within(
        zbus::proxy::Builder::<P>::new(connection)
            .destination(owner)?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build(),
    )
    .await
}

async fn night_light(
    connection: &Connection,
    owner: zbus::names::OwnedUniqueName,
) -> Result<(bool, String)> {
    let snapshot = within(
        owned_proxy::<NightLight1Proxy>(connection, owner)
            .await?
            .snapshot(),
    )
    .await?;
    Ok((snapshot.serving, snapshot.reason))
}

async fn weather(
    connection: &Connection,
    owner: zbus::names::OwnedUniqueName,
) -> Result<(bool, String)> {
    let (available, _stale, reason, ..) = within(
        owned_proxy::<Weather1Proxy>(connection, owner)
            .await?
            .snapshot(),
    )
    .await?;
    Ok((available, reason))
}

async fn calendar(
    connection: &Connection,
    owner: zbus::names::OwnedUniqueName,
) -> Result<(bool, String)> {
    let (reason, ..) = within(
        owned_proxy::<Calendar1Proxy>(connection, owner)
            .await?
            .snapshot(),
    )
    .await?;
    Ok((true, reason))
}

async fn notifications(
    connection: &Connection,
    owner: zbus::names::OwnedUniqueName,
) -> Result<(bool, String)> {
    let (_, _, serving, reason) = within(
        owned_proxy::<Notifications1Proxy>(connection, owner)
            .await?
            .snapshot(),
    )
    .await?;
    Ok((serving, reason))
}

async fn settings(
    connection: &Connection,
    owner: zbus::names::OwnedUniqueName,
) -> Result<(bool, String)> {
    let settings = owned_proxy::<Settings1Proxy>(connection, owner).await?;
    let (mode, location, solar) = within(async {
        tokio::try_join!(settings.mode(), settings.geolocation(), settings.solar())
    })
    .await?;
    decode_mode(mode.clone()).map_err(anyhow::Error::msg)?;
    decode_geolocation(location).map_err(anyhow::Error::msg)?;
    decode_solar(solar).map_err(anyhow::Error::msg)?;
    Ok((mode.0, mode.5))
}

async fn idle(
    connection: &Connection,
    owner: zbus::names::OwnedUniqueName,
) -> Result<(bool, String)> {
    let idle = owned_proxy::<Idle1Proxy>(connection, owner).await?;
    let (_, health) = within(async { tokio::try_join!(idle.inhibitors(), idle.health()) }).await?;
    let backends = [
        health.screen_saver,
        health.portal,
        health.login1,
        health.wayland,
    ];
    let degraded = backends
        .iter()
        .filter(|backend| backend.kind == HealthKind::Degraded)
        .collect::<Vec<_>>();
    if !degraded.is_empty() {
        return Ok((
            false,
            degraded
                .iter()
                .map(|backend| safe(&backend.message))
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }
    let ready = backends
        .iter()
        .any(|backend| backend.kind == HealthKind::Ready);
    Ok((
        ready,
        if ready {
            String::new()
        } else {
            "no idle inhibition backend is ready".into()
        },
    ))
}
