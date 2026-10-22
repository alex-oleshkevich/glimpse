use std::path::Path;

use anyhow::{Context as _, Result};
use glimpse_dbus::bluez::AgentManager1Proxy;
use glimpse_dbus::network_manager::AgentManagerProxy;

use crate::errors::ChecksFailed;
use glimpse_agents::polkit::helper::{SETUID, SOCKET};

struct Check {
    name: &'static str,
    outcome: Result<String, String>,
}

impl Check {
    fn line(&self) -> String {
        match &self.outcome {
            Ok(note) => format!("{}: ok ({note})", self.name),
            Err(reason) => format!("{}: fail ({reason})", self.name),
        }
    }
}

pub async fn check() -> Result<()> {
    let system = zbus::Connection::system()
        .await
        .context("connect to the system bus")?;
    let polkit =
        match tokio::time::timeout(glimpse_dbus::DEADLINE, zbus::Connection::session()).await {
            Ok(Ok(session)) => {
                glimpse_agents::polkit::agent::registration_report(&system, &session).await
            }
            Ok(Err(error)) => Err(format!("failed: {error}")),
            Err(_) => Err("failed: the session bus did not answer in time".to_owned()),
        };
    let checks = [
        Check {
            name: "polkit helper",
            outcome: helper_mode(),
        },
        Check {
            name: "polkit",
            outcome: polkit,
        },
        Check {
            name: "networkmanager",
            outcome: match AgentManagerProxy::new(&system).await {
                Ok(proxy) => ping(proxy.inner()).await,
                Err(error) => Err(error.to_string()),
            },
        },
        Check {
            name: "bluez",
            outcome: match AgentManager1Proxy::new(&system).await {
                Ok(proxy) => ping(proxy.inner()).await,
                Err(error) => Err(error.to_string()),
            },
        },
        Check {
            name: "logind",
            outcome: glimpse_agents::lock::session(&system)
                .await
                .map(|session| session.inner().path().to_string())
                .map_err(|error| format!("{error:#}")),
        },
    ];
    for check in &checks {
        println!("{}", check.line());
    }
    match checks.iter().filter(|check| check.outcome.is_err()).count() {
        0 => Ok(()),
        failed => Err(ChecksFailed(failed).into()),
    }
}

fn helper_mode() -> Result<String, String> {
    if Path::new(SOCKET).exists() {
        return Ok(format!("socket {SOCKET}"));
    }
    SETUID
        .iter()
        .find(|path| Path::new(path).exists())
        .map(|path| format!("setuid {path}"))
        .ok_or_else(|| "missing: no helper socket and no setuid helper".to_owned())
}

async fn ping(proxy: &zbus::Proxy<'_>) -> Result<String, String> {
    let answered = async {
        zbus::fdo::IntrospectableProxy::builder(proxy.connection())
            .destination(proxy.destination().to_owned())?
            .path(proxy.path().to_owned())?
            .build()
            .await?
            .introspect()
            .await
    };
    answered
        .await
        .map(|_| format!("{} answers", proxy.interface()))
        .map_err(|error| error.to_string())
}
