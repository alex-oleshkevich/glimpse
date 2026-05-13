use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use glimpse_core::{AppletConfig, AppletType, DiscoveredApplets};
use notify::EventKind;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::applets::exec;

struct DevChild {
    controller: Controller<exec::Applet>,
    cancel: CancellationToken,
}

pub struct Init {
    pub watcher: watch::Receiver<DiscoveredApplets>,
}

#[derive(Debug)]
pub enum Input {
    WatcherChanged(DiscoveredApplets),
    BinaryChanged { name: String },
}

pub struct Applet {
    children: HashMap<String, DevChild>,
    root: gtk::Box,
}

#[relm4::component(pub)]
impl SimpleComponent for Applet {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        root = gtk::Box {
            add_css_class: "applet",
            set_orientation: gtk::Orientation::Horizontal,
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();

        let mut watcher_rx = init.watcher.clone();
        let initial = watcher_rx.borrow_and_update().clone();
        let mut model = Applet {
            children: HashMap::new(),
            root: widgets.root.clone(),
        };
        model.reconcile(&initial, &widgets.root, &sender);

        let input_tx = sender.input_sender().clone();
        relm4::spawn(async move {
            while watcher_rx.changed().await.is_ok() {
                let discovered = watcher_rx.borrow_and_update().clone();
                if input_tx.send(Input::WatcherChanged(discovered)).is_err() {
                    break;
                }
            }
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Input::WatcherChanged(discovered) => {
                let root = self.root.clone();
                self.reconcile(&discovered, &root, &sender);
            }
            Input::BinaryChanged { name } => {
                if let Some(child) = self.children.get(&name) {
                    child.controller.emit(exec::Input::RestartCommand);
                    tracing::debug!(name, "dev applet binary changed, restarting");
                }
            }
        }
    }
}

impl Applet {
    fn reconcile(
        &mut self,
        discovered: &DiscoveredApplets,
        root: &gtk::Box,
        sender: &ComponentSender<Self>,
    ) {
        let new_dev = &discovered.dev;

        // Remove children no longer present.
        self.children.retain(|name, child| {
            if new_dev.contains_key(name) {
                true
            } else {
                tracing::info!(name, "dev applet removed");
                child.cancel.cancel();
                root.remove(child.controller.widget());
                false
            }
        });

        // Reconfigure existing children; add new ones in sorted order.
        let mut sorted: Vec<(&String, &PathBuf)> = new_dev.iter().collect();
        sorted.sort_by_key(|(name, _)| name.as_str());

        for (name, config_path) in sorted {
            if let Some(child) = self.children.get(name) {
                match parse_exec_config(config_path) {
                    Some(config) => child.controller.emit(exec::Input::Reconfigure(config)),
                    None => {
                        tracing::warn!(name, path = %config_path.display(), "could not parse dev applet config for reconfigure")
                    }
                }
            } else if let Some(child) = launch_child(name, config_path, sender) {
                tracing::info!(name, path = %config_path.display(), "dev applet launched");
                root.append(child.controller.widget());
                self.children.insert(name.clone(), child);
            }
        }
    }
}

fn launch_child(
    name: &str,
    config_path: &Path,
    sender: &ComponentSender<Applet>,
) -> Option<DevChild> {
    let config = parse_exec_config(config_path)?;
    if config.command.is_empty() {
        tracing::warn!(name, "dev applet has no command, skipping");
        return None;
    }

    let canonical_binary = fs::canonicalize(&config.command[0]).ok();

    let controller = exec::Applet::builder()
        .launch(exec::Init {
            name: name.to_string(),
            config,
        })
        .detach();

    let cancel = CancellationToken::new();

    if let Some(canonical) = canonical_binary {
        let name = name.to_string();
        let input_tx = sender.input_sender().clone();
        let token = cancel.clone();
        relm4::spawn(async move {
            watch_binary(name, canonical, input_tx, token).await;
        });
    }

    Some(DevChild { controller, cancel })
}

fn parse_exec_config(path: &Path) -> Option<exec::Config> {
    let content = fs::read_to_string(path)
        .map_err(|e| tracing::warn!(path = %path.display(), %e, "could not read dev applet config"))
        .ok()?;
    let settings: toml::Value = toml::from_str(&content)
        .map_err(
            |e| tracing::warn!(path = %path.display(), %e, "could not parse dev applet config"),
        )
        .ok()?;
    let applet_config = AppletConfig {
        extends: Some(AppletType::Exec),
        settings,
    };
    Some(exec::Config::from_raw(&Some(applet_config)))
}

async fn watch_binary(
    name: String,
    canonical: PathBuf,
    input_tx: relm4::Sender<Input>,
    cancel: CancellationToken,
) {
    let Some(dir) = canonical.parent().map(PathBuf::from) else {
        return;
    };

    let (handler_tx, mut handler_rx) = tokio::sync::mpsc::channel::<()>(1);
    let watch_canonical = canonical.clone();

    let mut debouncer = match new_debouncer(
        Duration::from_millis(300),
        None,
        move |res: DebounceEventResult| {
            let events = match res {
                Ok(e) => e,
                Err(_) => return,
            };
            let changed = events.iter().any(|e| {
                matches!(e.kind, EventKind::Modify(_) | EventKind::Create(_))
                    && e.paths.iter().any(|p| {
                        p == &watch_canonical
                            || p.canonicalize()
                                .map(|c| c == watch_canonical)
                                .unwrap_or(false)
                    })
            });
            if changed {
                let _ = handler_tx.try_send(());
            }
        },
    ) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(name, %e, "could not create binary watcher");
            return;
        }
    };

    if let Err(e) = debouncer.watch(&dir, notify::RecursiveMode::NonRecursive) {
        tracing::warn!(name, dir = %dir.display(), %e, "could not watch binary dir");
        return;
    }
    tracing::info!(name, binary = %canonical.display(), "watching dev applet binary for changes");

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            msg = handler_rx.recv() => match msg {
                Some(()) => {
                    if input_tx.send(Input::BinaryChanged { name: name.clone() }).is_err() {
                        break;
                    }
                }
                None => break,
            }
        }
    }
}
