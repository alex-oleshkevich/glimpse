use std::path::PathBuf;

use glimpse_config::Config;
use glimpse_contracts::KeyboardLayouts;
use glimpse_dbus::Buses;
use glimpse_ipc::Server;
use glimpse_services::{
    Calendar, Compositor, Geolocation, Heartbeat, Keyboard, KeyboardDependencies, Mpris,
    Notifications, Service, ServiceRuntime, Session, SessionDependencies, Solar, SolarDependencies,
    Weather, WeatherConfig, WeatherDependencies, initial_calendar_state, initial_compositor_state,
    initial_mpris_state, initial_notifications_state, initial_session_state, initial_weather_state,
};
use tokio::signal::unix::{SignalKind, signal};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::broker::{self, Message};
use crate::handler::BrokerHandler;
use crate::legacy_services;
use crate::reload::{self, ConfigSink};

#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error(transparent)]
    IpcServer(#[from] glimpse_ipc::ServerError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("load config: {0}")]
    Config(String),
    #[error("socket: {0}")]
    Socket(String),
    #[error("runtime: {0}")]
    Runtime(String),
}

const SERVICES: &[&str] = &[
    Geolocation::NAME,
    Solar::NAME,
    Heartbeat::NAME,
    Compositor::NAME,
    Keyboard::NAME,
    Calendar::NAME,
    Weather::NAME,
    Mpris::NAME,
    Notifications::NAME,
    Session::NAME,
];

#[derive(Default)]
pub struct Filter {
    pub only: Vec<String>,
    pub without: Vec<String>,
}

impl Filter {
    fn allows(&self, name: &str) -> bool {
        match self.only.is_empty() {
            false => self.only.iter().any(|wanted| wanted == name),
            true => !self.without.iter().any(|refused| refused == name),
        }
    }

    fn unmatched(&self) -> Vec<&String> {
        self.only
            .iter()
            .chain(&self.without)
            .filter(|name| !SERVICES.contains(&name.as_str()))
            .collect()
    }
}

pub struct Daemon {
    tasks: TaskTracker,
    filter: Filter,
}

impl Daemon {
    pub fn new(filter: Filter) -> Self {
        Self {
            tasks: TaskTracker::new(),
            filter,
        }
    }

    pub async fn run(
        self,
        socket: &PathBuf,
        config: Config,
        config_path: Option<PathBuf>,
    ) -> Result<(), DaemonError> {
        tracing::info!("daemon starting");
        for name in self.filter.unmatched() {
            tracing::warn!(service = %name, "no such service; the name matches nothing");
        }
        for name in SERVICES.iter().filter(|name| !self.filter.allows(name)) {
            tracing::info!(service = *name, "excluded by the command line");
        }

        let accepting = CancellationToken::new();
        let running = CancellationToken::new();
        let brokering = CancellationToken::new();
        let broker = broker::spawn(brokering.clone());
        let server = Server::bind(socket, BrokerHandler::new(broker.clone())).await?;
        let buses = Buses::connect().await;

        let (geolocation_runtime, geolocation) = ServiceRuntime::<Geolocation>::new(
            Geolocation::initial_state(),
            buses.clone(),
            running.child_token(),
        );
        let (solar_runtime, solar) = ServiceRuntime::<Solar>::new(
            Solar::initial_state(),
            buses.clone(),
            running.child_token(),
        );
        let (heartbeat_runtime, heartbeat) = ServiceRuntime::<Heartbeat>::new(
            Heartbeat::initial_state(),
            buses.clone(),
            running.child_token(),
        );
        let (compositor_runtime, compositor) = ServiceRuntime::<Compositor>::new(
            initial_compositor_state(),
            buses.clone(),
            running.child_token(),
        );
        let (keyboard_runtime, keyboard) = ServiceRuntime::<Keyboard>::new(
            KeyboardLayouts {
                layouts: Vec::new(),
                current: None,
            },
            buses.clone(),
            running.child_token(),
        );
        let (calendar_runtime, calendar) = ServiceRuntime::<Calendar>::new(
            initial_calendar_state(),
            buses.clone(),
            running.child_token(),
        );
        let (weather_runtime, weather) = ServiceRuntime::<Weather>::new(
            initial_weather_state(&WeatherConfig::from(&config)),
            buses.clone(),
            running.child_token(),
        );
        let (mpris_runtime, mpris) = ServiceRuntime::<Mpris>::new(
            initial_mpris_state(),
            buses.clone(),
            running.child_token(),
        );
        let (notifications_runtime, notifications) = ServiceRuntime::<Notifications>::new(
            initial_notifications_state(),
            buses.clone(),
            running.child_token(),
        );
        let (session_runtime, session) =
            ServiceRuntime::<Session>::new(initial_session_state(), buses, running.child_token());

        let mut sinks = Vec::new();
        if self.filter.allows(Geolocation::NAME) {
            legacy_services::geolocation(
                &self.tasks,
                &broker,
                running.child_token(),
                geolocation.clone(),
            );
            sinks.push(spawn_service(&self.tasks, &config, geolocation_runtime, ()));
        }
        if self.filter.allows(Solar::NAME) {
            legacy_services::solar(&self.tasks, &broker, running.child_token(), solar.clone());
            sinks.push(spawn_service(
                &self.tasks,
                &config,
                solar_runtime,
                SolarDependencies {
                    geolocation: geolocation.clone(),
                },
            ));
        }
        if self.filter.allows(Heartbeat::NAME) {
            legacy_services::heartbeat(
                &self.tasks,
                &broker,
                running.child_token(),
                heartbeat.clone(),
            );
            sinks.push(spawn_service(&self.tasks, &config, heartbeat_runtime, ()));
        }
        if self.filter.allows(Compositor::NAME) {
            legacy_services::compositor(
                &self.tasks,
                &broker,
                running.child_token(),
                compositor.clone(),
            );
            sinks.push(spawn_service(&self.tasks, &config, compositor_runtime, ()));
        }
        if self.filter.allows(Keyboard::NAME) {
            legacy_services::keyboard(
                &self.tasks,
                &broker,
                running.child_token(),
                keyboard.clone(),
            );
            sinks.push(spawn_service(
                &self.tasks,
                &config,
                keyboard_runtime,
                KeyboardDependencies {
                    compositor: compositor.clone(),
                },
            ));
        }
        if self.filter.allows(Calendar::NAME) {
            legacy_services::calendar(
                &self.tasks,
                &broker,
                running.child_token(),
                calendar.clone(),
            );
            sinks.push(spawn_service(&self.tasks, &config, calendar_runtime, ()));
        }
        if self.filter.allows(Weather::NAME) {
            legacy_services::weather(&self.tasks, &broker, running.child_token(), weather.clone());
            sinks.push(spawn_service(
                &self.tasks,
                &config,
                weather_runtime,
                WeatherDependencies {
                    geolocation: geolocation.clone(),
                },
            ));
        }
        if self.filter.allows(Mpris::NAME) {
            legacy_services::mpris(&self.tasks, &broker, running.child_token(), mpris.clone());
            sinks.push(spawn_service(&self.tasks, &config, mpris_runtime, ()));
        }
        if self.filter.allows(Notifications::NAME) {
            legacy_services::notifications(
                &self.tasks,
                &broker,
                running.child_token(),
                notifications.clone(),
            );
            sinks.push(spawn_service(
                &self.tasks,
                &config,
                notifications_runtime,
                (),
            ));
        }
        if self.filter.allows(Session::NAME) {
            legacy_services::session(&self.tasks, &broker, running.child_token(), session.clone());
            sinks.push(spawn_service(
                &self.tasks,
                &config,
                session_runtime,
                SessionDependencies { compositor },
            ));
        }

        self.tasks.spawn(reload::run(
            config_path,
            config,
            sinks,
            running.child_token(),
        ));
        broker.send(Message::SetPublisher(server.publisher()));
        tracing::info!(path=?socket, "daemon listening");
        let serving = tokio::spawn(server.serve(accepting.clone()));
        shutdown_signal().await?;

        accepting.cancel();
        if let Err(error) = serving.await {
            return Err(DaemonError::Runtime(error.to_string()));
        }

        running.cancel();
        self.tasks.close();
        self.tasks.wait().await;
        brokering.cancel();
        Ok(())
    }
}

fn spawn_service<S: Service>(
    tasks: &TaskTracker,
    document: &Config,
    mut runtime: ServiceRuntime<S>,
    dependencies: S::Dependencies,
) -> ConfigSink {
    let config = S::Config::from(document);
    let reconfigure = runtime.sender();
    let mut previous = config.clone();
    tasks.spawn(async move {
        if let Err(error) = runtime.run(config, dependencies).await {
            tracing::error!(service = S::NAME, %error, "service stopped");
        }
    });

    Box::new(move |document: &Config| {
        let next = S::Config::from(document);
        if next != previous {
            previous = next.clone();
            tracing::debug!(service = S::NAME, "reconfiguring");
            reconfigure.reconfigure(next);
        }
    })
}

async fn shutdown_signal() -> Result<(), DaemonError> {
    let mut terminate = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;

    tokio::select! {
        _ = terminate.recv() => {},
        _ = interrupt.recv() => {},
    }

    tracing::info!("shutting down");
    Ok(())
}
