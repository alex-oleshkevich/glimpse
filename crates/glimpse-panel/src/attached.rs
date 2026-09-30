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
mod tests {
    use super::*;
    use crate::services::PanelServices;
    use glimpse_dbus::Buses;
    use glimpse_dbus::applet_host::AppletHostProxy;
    use glimpse_dbus::testing::PrivateBus;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

    fn desktop() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "glimpse-panel-attached-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("me.x.A.desktop");
        fs::write(
            &path,
            "[Desktop Entry]\nType=Application\nName=Test\nExec=/bin/true\nImplements=me.aresa.Glimpse.Applet1;\n",
        )
        .unwrap();
        path
    }

    #[test]
    fn validation_and_caps_reject_bad_attachments() {
        let path = desktop();
        let path = path.to_str().unwrap();
        let layout = vec![vec!["DP-1".to_owned()]];
        let valid = || validate(path, 0, "", "", "{}", &layout).unwrap();
        assert_eq!(valid().output, "");
        assert!(validate(path, 99, "", "", "{}", &layout).is_err());
        assert!(validate(path, 0, "missing", "", "{}", &layout).is_err());
        assert!(validate(path, 0, "", "middle", "{}", &layout).is_err());
        assert!(validate("relative.desktop", 0, "", "", "{}", &layout).is_err());
        assert!(validate(&path.replace(".desktop", ".txt"), 0, "", "", "{}", &layout).is_err());
        let bad_stem = std::path::Path::new(path).with_file_name("bad id.desktop");
        fs::copy(path, &bad_stem).unwrap();
        assert!(validate(bad_stem.to_str().unwrap(), 0, "", "", "{}", &layout).is_err());
        let link = std::path::Path::new(path).with_file_name("me.x.Link.desktop");
        std::os::unix::fs::symlink(path, &link).unwrap();
        assert_eq!(
            validate(link.to_str().unwrap(), 0, "", "", "{}", &layout)
                .unwrap()
                .desktop_id,
            "me.x.Link"
        );
        assert!(validate(path, 0, "", "", "[1]", &layout).is_err());
        assert!(validate(path, 0, "", "", "{", &layout).is_err());
        assert!(matches!(
            check_options_size(&"x".repeat(64 * 1024 + 1)),
            Err(zbus::fdo::Error::InvalidArgs(message)) if message == "options are too large"
        ));

        let mut registry = AttachedRegistry::default();
        registry.set_layout(
            layout.clone(),
            vec![vec![("me.x.A".to_owned(), Zone::Center)]],
        );
        for _ in 0..8 {
            registry.insert("one".to_owned(), valid()).unwrap();
        }
        assert_eq!(registry.applets[0].zone, Zone::Center);
        assert!(registry.insert("one".to_owned(), valid()).is_err());
        for index in 0..8 {
            registry.insert(format!("other-{index}"), valid()).unwrap();
        }
        assert!(registry.insert("last".to_owned(), valid()).is_err());
        assert!(registry.remove("dev-1").is_some());
        assert!(registry.insert("one".to_owned(), valid()).is_ok());
        fs::remove_dir_all(std::path::Path::new(path).parent().unwrap()).unwrap();
    }

    #[test]
    fn reconcile_keeps_unmapped_named_output_and_moves_default_output() {
        let path = desktop();
        let layout = vec![vec!["DP-1".to_owned(), "HDMI-1".to_owned()]];
        let mut registry = AttachedRegistry::default();
        registry.set_layout(layout.clone(), vec![vec![]]);
        registry
            .insert(
                "named".to_owned(),
                validate(path.to_str().unwrap(), 0, "DP-1", "", "{}", &layout).unwrap(),
            )
            .unwrap();
        registry
            .insert(
                "default".to_owned(),
                validate(path.to_str().unwrap(), 0, "", "", "{}", &layout).unwrap(),
            )
            .unwrap();
        let stale = registry
            .insert(
                "stale".to_owned(),
                validate(path.to_str().unwrap(), 0, "", "", "{}", &layout).unwrap(),
            )
            .unwrap();
        registry.offered(&stale);
        assert_eq!(registry.applets.len(), 1);
        registry.remove(&stale);
        registry
            .insert(
                "named".to_owned(),
                validate(path.to_str().unwrap(), 0, "DP-1", "", "{}", &layout).unwrap(),
            )
            .unwrap();
        registry
            .insert(
                "default".to_owned(),
                validate(path.to_str().unwrap(), 0, "", "", "{}", &layout).unwrap(),
            )
            .unwrap();
        for applet in &mut registry.applets {
            applet.offered = true;
        }
        assert_eq!(registry.applets[1].output, "");
        assert_eq!(visible_on(&registry.applets, &layout, 0, "DP-1").len(), 2);

        let unmapped = vec![vec!["HDMI-1".to_owned()]];
        registry.set_layout(unmapped.clone(), vec![vec![]]);
        assert_eq!(registry.applets.len(), 2);
        assert_eq!(visible_on(&registry.applets, &unmapped, 0, "DP-1").len(), 0);
        assert_eq!(
            visible_on(&registry.applets, &unmapped, 0, "HDMI-1")[0].1,
            "me.x.A"
        );

        registry.set_layout(layout.clone(), vec![vec![]]);
        let remapped = visible_on(&registry.applets, &layout, 0, "DP-1");
        assert_eq!(remapped.len(), 2);
        assert_eq!(remapped[0].1, "me.x.A");
        assert_eq!(remapped[1].1, "me.x.A");
        assert!(visible_on(&registry.applets, &layout, 0, "HDMI-1").is_empty());
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn private_bus_checks_sender_and_delivers_fds() {
        let bus = PrivateBus::start();
        let server = bus.connection().await;
        let first = bus.connection().await;
        let second = bus.connection().await;
        let registry = SharedRegistry::default();
        registry
            .lock()
            .unwrap()
            .set_layout(vec![vec!["DP-1".to_owned()]], vec![vec![]]);
        let services = PanelServices::start_with_buses(
            &glimpse_config::Config::default(),
            Buses::unavailable("no bus in tests"),
        );
        let exec = services.exec();
        let (input, receiver) = relm4::channel();
        export(&server, registry.clone(), exec.clone(), input).await;
        let name = glimpse_dbus::applet_host_name(&glimpse_dbus::panel_app_id());
        let first = AppletHostProxy::builder(&first)
            .destination(name.clone())
            .unwrap()
            .build()
            .await
            .unwrap();
        let second = AppletHostProxy::builder(&second)
            .destination(name)
            .unwrap()
            .build()
            .await
            .unwrap();
        let path = desktop();
        let (to_panel, _writer) = std::os::unix::net::UnixStream::pair().unwrap();
        let (_reader, from_panel) = std::os::unix::net::UnixStream::pair().unwrap();
        let handle = first
            .attach(
                path.to_str().unwrap(),
                0,
                "",
                "",
                "{}",
                Fd::from(&to_panel),
                Fd::from(&from_panel),
            )
            .await
            .unwrap();
        assert_eq!(handle, "dev-1");
        assert!(matches!(receiver.recv().await, Some(AppInput::Attached)));
        assert!(exec.snapshot().offers.contains("me.x.A"));
        let (duplicate_to_panel, _duplicate_writer) =
            std::os::unix::net::UnixStream::pair().unwrap();
        let (_duplicate_reader, duplicate_from_panel) =
            std::os::unix::net::UnixStream::pair().unwrap();
        let error = second
            .attach(
                path.to_str().unwrap(),
                0,
                "",
                "",
                "{}",
                Fd::from(&duplicate_to_panel),
                Fd::from(&duplicate_from_panel),
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("me.x.A is already attached"));
        assert_eq!(registry.lock().unwrap().applets.len(), 1);
        let error = second.set_options(&handle, "{}").await.unwrap_err();
        assert!(error.to_string().contains("AccessDenied"));
        let error = first.set_options("unknown", "{}").await.unwrap_err();
        assert!(error.to_string().contains("AccessDenied"));
        services.shutdown().await;
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
