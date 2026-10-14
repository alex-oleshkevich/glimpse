use std::collections::{HashMap, HashSet};
use std::os::fd::AsFd;
use std::path::Path;
use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use glimpse_dbus::applet_host::APPLET_HOST_PATH;
use glimpse_services::{DesktopCatalog, Edge, ExecDevRequest, ExecEntry, ExecHandle, Zone};
use serde_json::{Map, Value};
use zbus::zvariant::Fd;

use crate::app::AppInput;

#[derive(Clone)]
pub struct AttachedApplet {
    pub handle: String,
    pub sender: String,
    pub desktop_id: String,
    pub panel: usize,
    pub output: String,
    pub zone: Zone,
}

#[derive(Default)]
pub struct AttachedRegistry {
    pub applets: Vec<AttachedApplet>,
    layout: Vec<Vec<String>>,
    configured: Vec<Vec<(String, Zone)>>,
    edges: Vec<Edge>,
}

pub type SharedRegistry = Arc<Mutex<AttachedRegistry>>;

#[derive(Clone)]
struct Validated {
    desktop_id: String,
    entry: ExecEntry,
    panel: usize,
    output: String,
    zone: Option<Zone>,
    options: Map<String, Value>,
}

fn validate(
    desktop_file: &str,
    panel: u32,
    output: &str,
    zone: &str,
    options: &str,
    layout: &[Vec<String>],
) -> Result<Validated, String> {
    let path = Path::new(desktop_file);
    if !path.is_absolute() {
        return Err("desktop file must be absolute".to_owned());
    }
    if path
        .extension()
        .is_none_or(|extension| extension != "desktop")
    {
        return Err("desktop file must end in .desktop".to_owned());
    }
    let desktop_id = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| glimpse_config::is_desktop_id(stem))
        .ok_or_else(|| "invalid desktop id".to_owned())?
        .to_owned();
    let path = path.canonicalize().map_err(|error| error.to_string())?;
    if !path.is_file() {
        return Err("desktop file must be a regular file".to_owned());
    }
    let panel = panel as usize;
    let outputs = layout
        .get(panel)
        .ok_or_else(|| "unknown panel".to_owned())?;
    if !outputs
        .iter()
        .any(|mapped| output.is_empty() || mapped == output)
    {
        return Err("unknown output".to_owned());
    }
    let zone = match zone {
        "" => None,
        "left" => Some(Zone::Left),
        "center" => Some(Zone::Center),
        "right" => Some(Zone::Right),
        _ => return Err("unknown zone".to_owned()),
    };
    let options = match serde_json::from_str(options).map_err(|error| error.to_string())? {
        Value::Object(options) => options,
        _ => return Err("options must be a JSON object".to_owned()),
    };
    let entry = DesktopCatalog.resolve_file(&path)?;
    Ok(Validated {
        desktop_id,
        entry,
        panel,
        output: output.to_owned(),
        zone,
        options,
    })
}

fn check_options_size(options: &str) -> zbus::fdo::Result<()> {
    if options.len() > 64 * 1024 {
        return Err(zbus::fdo::Error::InvalidArgs(
            "options are too large".to_owned(),
        ));
    }
    Ok(())
}

impl AttachedRegistry {
    pub fn set_layout(&mut self, layout: Vec<Vec<String>>, configured: Vec<Vec<(String, Zone)>>) {
        self.layout = layout;
        self.configured = configured;
    }

    pub fn set_edges(&mut self, edges: Vec<Edge>) {
        self.edges = edges;
    }

    fn admit(&self, sender: &str, valid: &Validated) -> Result<(), String> {
        if !self.layout.get(valid.panel).is_some_and(|outputs| {
            if valid.output.is_empty() {
                !outputs.is_empty()
            } else {
                outputs.contains(&valid.output)
            }
        }) {
            return Err("unknown output".to_owned());
        }
        if self
            .applets
            .iter()
            .any(|applet| applet.handle == valid.desktop_id)
        {
            return Err(format!("{} is already attached", valid.desktop_id));
        }
        if self
            .applets
            .iter()
            .filter(|applet| applet.sender == sender)
            .count()
            >= 8
        {
            return Err("sender attachment limit reached".to_owned());
        }
        if self.applets.len() >= 16 {
            return Err("panel attachment limit reached".to_owned());
        }
        Ok(())
    }

    fn insert(&mut self, sender: String, handle: String, valid: Validated) -> Result<(), String> {
        self.admit(&sender, &valid)?;
        let zone = valid.zone.unwrap_or_else(|| {
            self.configured
                .get(valid.panel)
                .and_then(|configured| configured.iter().find(|(id, _)| id == &valid.desktop_id))
                .map_or(Zone::Right, |(_, zone)| *zone)
        });
        self.applets.push(AttachedApplet {
            handle,
            sender,
            desktop_id: valid.desktop_id,
            panel: valid.panel,
            output: valid.output,
            zone,
        });
        Ok(())
    }

    pub fn remove(&mut self, handle: &str) -> Option<AttachedApplet> {
        let index = self
            .applets
            .iter()
            .position(|applet| applet.handle == handle)?;
        let applet = self.applets.remove(index);
        tracing::info!(handle = %applet.handle, applet_id = %applet.desktop_id, "attached applet removed");
        Some(applet)
    }

    fn placement_output(&self, panel: usize, output: &str) -> Option<String> {
        if output.is_empty() {
            self.layout.get(panel)?.first().cloned()
        } else {
            Some(output.to_owned())
        }
    }
}

fn output_for<'a>(layout: &'a [Vec<String>], applet: &'a AttachedApplet) -> Option<&'a str> {
    if applet.output.is_empty() {
        layout.get(applet.panel)?.first().map(String::as_str)
    } else {
        Some(&applet.output)
    }
}

pub fn visible_on(
    applets: &[AttachedApplet],
    layout: &[Vec<String>],
    panel: usize,
    connector: &str,
) -> Vec<(crate::components::panel::Zone, String)> {
    if !layout
        .get(panel)
        .is_some_and(|outputs| outputs.iter().any(|output| output == connector))
    {
        return Vec::new();
    }
    applets
        .iter()
        .filter(|applet| {
            applet.panel == panel
                && output_for(layout, applet).is_some_and(|output| output == connector)
        })
        .map(|applet| {
            let zone = match applet.zone {
                Zone::Left => crate::components::panel::Zone::Start,
                Zone::Center => crate::components::panel::Zone::Center,
                Zone::Right => crate::components::panel::Zone::End,
            };
            (zone, applet.desktop_id.clone())
        })
        .collect()
}

pub struct AppletHost {
    pub registry: SharedRegistry,
    pub exec: ExecHandle,
    pub input: relm4::Sender<AppInput>,
    watching: Arc<Mutex<HashSet<String>>>,
}

impl AppletHost {
    fn owned(&self, handle: &str, sender: &str) -> zbus::fdo::Result<()> {
        let owns = self
            .registry
            .lock()
            .map_err(poisoned)?
            .applets
            .iter()
            .any(|applet| applet.handle == handle && applet.sender == sender);
        if owns {
            Ok(())
        } else {
            Err(zbus::fdo::Error::AccessDenied(
                "unknown handle or sender".to_owned(),
            ))
        }
    }

    fn watch_sender(&self, connection: &zbus::Connection, sender: &str) {
        if !self
            .watching
            .lock()
            .is_ok_and(|mut watching| watching.insert(sender.to_owned()))
        {
            return;
        }
        let connection = connection.clone();
        let sender = sender.to_owned();
        let registry = self.registry.clone();
        let exec = self.exec.clone();
        let input = self.input.clone();
        let watching = self.watching.clone();
        relm4::spawn(async move {
            wait_for_vanish(&connection, &sender).await;
            let handles: Vec<String> = registry.lock().map_or_else(
                |_| Vec::new(),
                |registry| {
                    registry
                        .applets
                        .iter()
                        .filter(|applet| applet.sender == sender)
                        .map(|applet| applet.handle.clone())
                        .collect()
                },
            );
            for handle in handles {
                if let Err(error) = exec.detach_dev(&handle).await {
                    tracing::debug!(%error, %handle, "detach after sender vanished");
                }
                if let Ok(mut registry) = registry.lock() {
                    registry.remove(&handle);
                }
                let _ = input.send(AppInput::Detached(handle));
            }
            if let Ok(mut watching) = watching.lock() {
                watching.remove(&sender);
            }
        });
    }
}

async fn wait_for_vanish(connection: &zbus::Connection, sender: &str) {
    let Ok(name) = zbus::names::BusName::try_from(sender.to_owned()) else {
        return;
    };
    let Ok(dbus) = zbus::fdo::DBusProxy::new(connection).await else {
        return;
    };
    let Ok(mut changes) = dbus
        .receive_name_owner_changed_with_args(&[(0, sender)])
        .await
    else {
        return;
    };
    if !dbus.name_has_owner(name).await.unwrap_or(false) {
        return;
    }
    while let Some(change) = changes.next().await {
        if change.args().is_ok_and(|args| args.new_owner().is_none()) {
            return;
        }
    }
}

#[zbus::interface(name = "me.aresa.Glimpse.AppletHost1")]
impl AppletHost {
    #[allow(clippy::too_many_arguments)]
    async fn attach(
        &self,
        desktop_path: &str,
        panel: u32,
        output: &str,
        zone: &str,
        options_json: &str,
        env: HashMap<String, String>,
        stderr: Fd<'_>,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> zbus::fdo::Result<String> {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        let applet_id = Path::new(desktop_path)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("");
        let result: zbus::fdo::Result<String> = async {
            check_options_size(options_json)?;
            let stderr = stderr
                .as_fd()
                .try_clone_to_owned()
                .map_err(|error| zbus::fdo::Error::InvalidArgs(error.to_string()))?;
            let layout = self.registry.lock().map_err(poisoned)?.layout.clone();
            let path = desktop_path.to_owned();
            let requested_output = output.to_owned();
            let requested_zone = zone.to_owned();
            let options = options_json.to_owned();
            let valid = tokio::task::spawn_blocking(move || {
                validate(
                    &path,
                    panel,
                    &requested_output,
                    &requested_zone,
                    &options,
                    &layout,
                )
            })
            .await
            .map_err(|error| zbus::fdo::Error::InvalidArgs(error.to_string()))?
            .map_err(zbus::fdo::Error::InvalidArgs)?;
            let (placed, position) = {
                let registry = self.registry.lock().map_err(poisoned)?;
                registry
                    .admit(&sender, &valid)
                    .map_err(zbus::fdo::Error::InvalidArgs)?;
                (
                    registry.placement_output(valid.panel, &valid.output),
                    registry.edges.get(valid.panel).copied(),
                )
            };
            let request = ExecDevRequest {
                entry: valid.entry.clone(),
                output: placed,
                position,
                options: valid.options.clone(),
                env: env.into_iter().collect(),
                stderr: Some(stderr),
            };
            let handle = self
                .exec
                .attach_dev(&valid.desktop_id, request)
                .await
                .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;
            let inserted = self.registry.lock().map_err(poisoned)?.insert(
                sender.clone(),
                handle.clone(),
                valid,
            );
            if let Err(reason) = inserted {
                let _ = self.exec.detach_dev(&handle).await;
                return Err(zbus::fdo::Error::InvalidArgs(reason));
            }
            self.watch_sender(connection, &sender);
            if self.input.send(AppInput::Attached).is_err() {
                let _ = self.exec.detach_dev(&handle).await;
                self.registry.lock().map_err(poisoned)?.remove(&handle);
                return Err(zbus::fdo::Error::Failed("panel is stopping".to_owned()));
            }
            Ok(handle)
        }
        .await;
        match &result {
            Ok(handle) => {
                tracing::info!(%sender, applet_id, panel, output, zone, %handle, "attached applet")
            }
            Err(error) => {
                tracing::info!(%sender, applet_id, panel, output, zone, reason = %error, "applet attach refused")
            }
        }
        result
    }

    async fn restart(
        &self,
        handle: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<()> {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        self.owned(handle, &sender)?;
        self.exec
            .restart_dev(handle)
            .await
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
    }

    async fn detach(
        &self,
        handle: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<()> {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        self.owned(handle, &sender)?;
        let result = self
            .exec
            .detach_dev(handle)
            .await
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()));
        self.registry.lock().map_err(poisoned)?.remove(handle);
        let _ = self.input.send(AppInput::Detached(handle.to_owned()));
        result
    }

    async fn set_options(
        &self,
        handle: &str,
        options_json: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<()> {
        check_options_size(options_json)?;
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        self.owned(handle, &sender)?;
        let options = match serde_json::from_str(options_json)
            .map_err(|error| zbus::fdo::Error::InvalidArgs(error.to_string()))?
        {
            Value::Object(options) => options,
            _ => {
                return Err(zbus::fdo::Error::InvalidArgs(
                    "options must be a JSON object".to_owned(),
                ));
            }
        };
        self.exec
            .dev_set_options(handle, options)
            .await
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
    }
}

fn poisoned<T>(_: std::sync::PoisonError<T>) -> zbus::fdo::Error {
    zbus::fdo::Error::Failed("attached registry is poisoned".to_owned())
}

pub async fn export(
    connection: &zbus::Connection,
    registry: SharedRegistry,
    exec: ExecHandle,
    input: relm4::Sender<AppInput>,
) {
    if let Err(error) = connection
        .object_server()
        .at(
            APPLET_HOST_PATH,
            AppletHost {
                registry,
                exec,
                input,
                watching: Arc::default(),
            },
        )
        .await
    {
        tracing::warn!(%error, "applet host export failed");
        return;
    }
    let name = glimpse_dbus::applet_host_name(&glimpse_dbus::panel_app_id());
    if let Err(error) = glimpse_dbus::own_name(connection, &name).await {
        tracing::warn!(%error, %name, "applet host name unavailable");
    }
}

#[cfg(test)]
mod tests;
