use std::collections::HashMap;
use std::os::fd::AsFd;
use std::path::Path;
use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use glimpse_dbus::applet_host::APPLET_HOST_PATH;
use glimpse_services::{DesktopCatalog, Edge, ExecDevRequest, ExecEntry, ExecHandle, Zone};
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;
use zbus::zvariant::Fd;

use crate::app::AppInput;

#[derive(Clone)]
pub struct AttachedApplet {
    pub handle: String,
    pub sender: String,
    pub desktop_id: String,
    pub dev_key: String,
    pub panel: usize,
    pub output: String,
    pub zone: Zone,
    pub stop: CancellationToken,
}

#[derive(Default)]
pub struct AttachedRegistry {
    pub applets: Vec<AttachedApplet>,
    layout: Vec<Vec<String>>,
    configured: Vec<Vec<(String, Zone)>>,
    edges: Vec<Edge>,
    attachments: u64,
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
        let outputs = self
            .layout
            .get(valid.panel)
            .ok_or_else(|| "unknown panel".to_owned())?;
        if !(if valid.output.is_empty() {
            !outputs.is_empty()
        } else {
            outputs.contains(&valid.output)
        }) {
            return Err("unknown output".to_owned());
        }
        if self
            .applets
            .iter()
            .any(|applet| applet.desktop_id == valid.desktop_id)
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

    fn insert(
        &mut self,
        sender: String,
        dev_key: String,
        stop: CancellationToken,
        valid: Validated,
    ) -> Result<String, String> {
        self.admit(&sender, &valid)?;
        self.attachments += 1;
        let handle = format!("{}#{}", valid.desktop_id, self.attachments);
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
            dev_key,
            panel: valid.panel,
            output: valid.output,
            zone,
            stop,
        });
        Ok(handle)
    }

    pub fn remove(&mut self, handle: &str) -> Option<AttachedApplet> {
        self.take(|applet| applet.handle == handle)
    }

    fn take(&mut self, matches: impl Fn(&AttachedApplet) -> bool) -> Option<AttachedApplet> {
        let index = self.applets.iter().position(matches)?;
        let applet = self.applets.remove(index);
        applet.stop.cancel();
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
}

impl AppletHost {
    fn owned(&self, handle: &str, sender: &str) -> zbus::fdo::Result<String> {
        self.registry
            .lock()
            .map_err(poisoned)?
            .applets
            .iter()
            .find(|applet| applet.handle == handle && applet.sender == sender)
            .map(|applet| applet.dev_key.clone())
            .ok_or_else(unknown_handle)
    }

    fn watch_vanish(&self, vanish: Vanish, handle: String, stop: CancellationToken) {
        let registry = self.registry.clone();
        let exec = self.exec.clone();
        let input = self.input.clone();
        relm4::spawn(async move {
            tokio::select! {
                () = stop.cancelled() => return,
                () = vanish.wait() => {}
            }
            let taken = registry
                .lock()
                .ok()
                .and_then(|mut registry| registry.remove(&handle));
            let Some(applet) = taken else { return };
            if let Err(error) = exec.detach_dev(&applet.dev_key).await {
                tracing::debug!(%error, %handle, "detach after sender vanished");
            }
            let _ = input.send(AppInput::Detached);
        });
    }
}

struct Vanish {
    dbus: zbus::fdo::DBusProxy<'static>,
    changes: futures_util::stream::BoxStream<'static, glimpse_dbus::Delivery<bool>>,
    name: zbus::names::BusName<'static>,
}

impl Vanish {
    async fn subscribe(connection: &zbus::Connection, sender: &str) -> zbus::fdo::Result<Self> {
        let name = zbus::names::BusName::try_from(sender.to_owned())
            .map_err(|error| zbus::fdo::Error::InvalidArgs(error.to_string()))?;
        let unwatchable = |error: zbus::Error| {
            zbus::fdo::Error::Failed(format!("cannot watch {sender}: {error}"))
        };
        let dbus = zbus::fdo::DBusProxy::new(connection)
            .await
            .map_err(unwatchable)?;
        let changes = dbus
            .receive_name_owner_changed_with_args(&[(0, sender)])
            .await
            .map(|stream| {
                glimpse_dbus::events(stream.filter_map(|change| async move {
                    change.args().ok().map(|args| args.new_owner().is_none())
                }))
            })
            .map_err(unwatchable)?;
        Ok(Self {
            dbus,
            changes,
            name,
        })
    }

    async fn wait(mut self) {
        if gone(&self.dbus, &self.name).await {
            return;
        }
        while let Some(change) = self.changes.next().await {
            let vanished = match change {
                glimpse_dbus::Delivery::Item(vanished) => vanished,
                glimpse_dbus::Delivery::Missed => gone(&self.dbus, &self.name).await,
            };
            if vanished {
                return;
            }
        }
    }
}

async fn gone(dbus: &zbus::fdo::DBusProxy<'_>, name: &zbus::names::BusName<'_>) -> bool {
    !dbus.name_has_owner(name.to_owned()).await.unwrap_or(true)
}

fn unknown_handle() -> zbus::fdo::Error {
    zbus::fdo::Error::AccessDenied("unknown handle or sender".to_owned())
}

fn check_env(env: &HashMap<String, String>) -> zbus::fdo::Result<()> {
    match env
        .keys()
        .find(|name| !glimpse_config::exec_env_name_allowed(name))
    {
        Some(name) => Err(zbus::fdo::Error::InvalidArgs(format!(
            "env name is not allowed: {name:?}"
        ))),
        None => Ok(()),
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
            check_env(&env)?;
            let stderr = stderr
                .as_fd()
                .try_clone_to_owned()
                .map_err(|error| zbus::fdo::Error::InvalidArgs(error.to_string()))?;
            let path = desktop_path.to_owned();
            let requested_output = output.to_owned();
            let requested_zone = zone.to_owned();
            let options = options_json.to_owned();
            let valid = tokio::task::spawn_blocking(move || {
                validate(&path, panel, &requested_output, &requested_zone, &options)
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
            let vanish = Vanish::subscribe(connection, &sender).await?;
            let dev_key = self
                .exec
                .attach_dev(&valid.desktop_id, request)
                .await
                .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;
            let stop = CancellationToken::new();
            let inserted = self.registry.lock().map_err(poisoned)?.insert(
                sender.clone(),
                dev_key.clone(),
                stop.clone(),
                valid,
            );
            let handle = match inserted {
                Ok(handle) => handle,
                Err(reason) => {
                    let _ = self.exec.detach_dev(&dev_key).await;
                    return Err(zbus::fdo::Error::InvalidArgs(reason));
                }
            };
            self.watch_vanish(vanish, handle.clone(), stop);
            if self.input.send(AppInput::Attached).is_err() {
                if self
                    .registry
                    .lock()
                    .map_err(poisoned)?
                    .remove(&handle)
                    .is_some()
                {
                    let _ = self.exec.detach_dev(&dev_key).await;
                }
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
        let dev_key = self.owned(handle, &sender)?;
        self.exec
            .restart_dev(&dev_key)
            .await
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
    }

    async fn tree(
        &self,
        handle: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<String> {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        let dev_key = self.owned(handle, &sender)?;
        let tree = self
            .exec
            .dev_tree(&dev_key)
            .await
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;
        serde_json::to_string(&tree).map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
    }

    async fn detach(
        &self,
        handle: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<()> {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        let applet = self
            .registry
            .lock()
            .map_err(poisoned)?
            .take(|applet| applet.handle == handle && applet.sender == sender)
            .ok_or_else(unknown_handle)?;
        let result = self
            .exec
            .detach_dev(&applet.dev_key)
            .await
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()));
        let _ = self.input.send(AppInput::Detached);
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
        let dev_key = self.owned(handle, &sender)?;
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
            .dev_set_options(&dev_key, options)
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
