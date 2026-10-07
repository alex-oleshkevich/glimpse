use std::os::fd::AsFd;
use std::path::Path;
use std::sync::{Arc, Mutex};

use glimpse_dbus::applet_host::APPLET_HOST_PATH;
use glimpse_services::{DesktopCatalog, Edge, ExecEntry, ExecHandle, ExecOffer, ExecPipes, Zone};
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
    pub offered: bool,
}

#[derive(Default)]
pub struct AttachedRegistry {
    pub applets: Vec<AttachedApplet>,
    next: u64,
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

    fn insert(&mut self, sender: String, valid: Validated) -> Result<String, String> {
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
            .filter(|applet| applet.sender == sender)
            .count()
            >= 8
        {
            return Err("sender attachment limit reached".to_owned());
        }
        if self.applets.len() >= 16 {
            return Err("panel attachment limit reached".to_owned());
        }
        self.next += 1;
        let handle = format!("dev-{}", self.next);
        let zone = valid.zone.unwrap_or_else(|| {
            self.configured
                .get(valid.panel)
                .and_then(|configured| configured.iter().find(|(id, _)| id == &valid.desktop_id))
                .map_or(Zone::Right, |(_, zone)| *zone)
        });
        self.applets.push(AttachedApplet {
            handle: handle.clone(),
            sender,
            desktop_id: valid.desktop_id,
            panel: valid.panel,
            output: valid.output,
            zone,
            offered: false,
        });
        Ok(handle)
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

    pub fn offered(&mut self, handle: &str) {
        let Some(applet) = self
            .applets
            .iter_mut()
            .find(|applet| applet.handle == handle)
        else {
            return;
        };
        applet.offered = true;
        let id = applet.desktop_id.clone();
        self.applets.retain(|applet| {
            let keep = applet.handle == handle || applet.desktop_id != id;
            if !keep {
                tracing::info!(handle = %applet.handle, applet_id = %applet.desktop_id, "attached applet removed");
            }
            keep
        });
    }

    pub fn placement_output(&self, applet: &AttachedApplet) -> Option<String> {
        output_for(&self.layout, applet).map(str::to_owned)
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
            applet.offered
                && applet.panel == panel
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
}

#[zbus::interface(name = "me.aresa.Glimpse.AppletHost1")]
impl AppletHost {
    #[allow(clippy::too_many_arguments)]
    async fn attach(
        &self,
        desktop_file: &str,
        panel: u32,
        output: &str,
        zone: &str,
        options: &str,
        grants: &str,
        to_applet: Fd<'_>,
        from_applet: Fd<'_>,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<String> {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        let applet_id = Path::new(desktop_file)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("");
        let result: zbus::fdo::Result<String> = async {
            check_options_size(options)?;
            check_options_size(grants)?;
            let grants: glimpse_config::AppletSandbox = serde_json::from_str(grants)
                .map_err(|error| zbus::fdo::Error::InvalidArgs(error.to_string()))?;
            let layout = self.registry.lock().map_err(poisoned)?.layout.clone();
            let desktop_file = desktop_file.to_owned();
            let output = output.to_owned();
            let zone = zone.to_owned();
            let options = options.to_owned();
            let valid = tokio::task::spawn_blocking(move || {
                validate(&desktop_file, panel, &output, &zone, &options, &layout)
            })
            .await
            .map_err(|error| zbus::fdo::Error::InvalidArgs(error.to_string()))?
            .map_err(zbus::fdo::Error::InvalidArgs)?;
            let pipes = ExecPipes {
                to_applet: to_applet
                    .as_fd()
                    .try_clone_to_owned()
                    .map_err(|error| zbus::fdo::Error::InvalidArgs(error.to_string()))?,
                from_applet: from_applet
                    .as_fd()
                    .try_clone_to_owned()
                    .map_err(|error| zbus::fdo::Error::InvalidArgs(error.to_string()))?,
            };
            let id = valid.desktop_id.clone();
            let (handle, output, position) = {
                let mut registry = self.registry.lock().map_err(poisoned)?;
                let handle = registry
                    .insert(sender.clone(), valid.clone())
                    .map_err(zbus::fdo::Error::InvalidArgs)?;
                let output = registry
                    .applets
                    .last()
                    .and_then(|applet| registry.placement_output(applet));
                (handle, output, registry.edges.get(valid.panel).copied())
            };
            let offer = ExecOffer {
                output,
                position,
                entry: valid.entry.clone(),
                options: valid.options.clone(),
                sandbox: grants,
                pipes,
            };
            if let Err(error) = self.exec.offer(&id, offer).await {
                self.registry.lock().map_err(poisoned)?.remove(&handle);
                return Err(zbus::fdo::Error::Failed(error.to_string()));
            }
            self.registry.lock().map_err(poisoned)?.offered(&handle);
            if self.input.send(AppInput::Attached).is_err() {
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

    async fn set_options(
        &self,
        handle: &str,
        options: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<()> {
        check_options_size(options)?;
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        let owns = |registry: &AttachedRegistry| {
            registry
                .applets
                .iter()
                .any(|applet| applet.handle == handle && applet.sender == sender)
        };
        let denied = || zbus::fdo::Error::AccessDenied("unknown handle or sender".to_owned());
        if !owns(&*self.registry.lock().map_err(poisoned)?) {
            return Err(denied());
        }
        let options = match serde_json::from_str(options)
            .map_err(|error| zbus::fdo::Error::InvalidArgs(error.to_string()))?
        {
            Value::Object(options) => options,
            _ => {
                return Err(zbus::fdo::Error::InvalidArgs(
                    "options must be a JSON object".to_owned(),
                ));
            }
        };
        let mut registry = self.registry.lock().map_err(poisoned)?;
        let applet = registry
            .applets
            .iter_mut()
            .find(|applet| applet.handle == handle && applet.sender == sender)
            .ok_or_else(denied)?;
        let id = applet.desktop_id.clone();
        drop(registry);
        self.input
            .send(AppInput::OfferOptions { id, options })
            .map_err(|_| zbus::fdo::Error::Failed("panel is stopping".to_owned()))
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
