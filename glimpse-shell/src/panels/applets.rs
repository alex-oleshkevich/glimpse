use relm4::{
    Component, ComponentController, Controller,
    gtk::{
        self,
        glib::object::{Cast, CastNone},
        prelude::{BoxExt, WidgetExt},
    },
};
use std::collections::HashMap;

use crate::{
    applets::{
        audio, battery, bluetooth, brightness, clipboard, clock, command, display, dynamic, exec,
        idle, keyboard, mpris, network, next_event, notifications, pager, printing, privacy,
        removable, session, tray, weather, window, workspace,
    },
    panels::PanelSection,
    services::{framework::Services, wayland_idle_inhibit::SHELL_EXTENSIONS},
};

use glimpse_core::ThemeMode;
use glimpse_core::ipc::IpcEmitter;
pub use glimpse_core::{AppletConfig, AppletType};

fn applet_type_name(applet_type: AppletType) -> &'static str {
    applet_type.as_config_name()
}

fn section_name(section: &PanelSection) -> &'static str {
    match section {
        PanelSection::Left => "left",
        PanelSection::Center => "center",
        PanelSection::Right => "right",
    }
}

fn emit_applet(
    ipc: &IpcEmitter,
    event: &str,
    monitor: &str,
    key: &AppletKey,
    applet_type: AppletType,
) {
    ipc.emit(
        event,
        vec![
            ("monitor", monitor.to_owned()),
            ("section", section_name(&key.section).to_owned()),
            ("name", key.name.clone()),
            ("type", applet_type_name(applet_type).to_owned()),
            ("occurrence", key.occurrence.to_string()),
        ],
    );
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AppletKey {
    pub section: PanelSection,
    pub name: String,
    pub occurrence: usize,
}

#[derive(Debug, Clone)]
pub struct AppletBlueprint {
    pub key: AppletKey,
    pub name: String,
    pub applet_type: AppletType,
    pub config: Option<AppletConfig>,
}

macro_rules! define_applet_controller {
    // Private arms: reconfigure dispatch by style tag
    (@std $mod:ident, $controller:ident, $config:ident, $_theme:ident) => {
        $controller.emit($mod::Input::Reconfigure($mod::Config::from_raw(&$config.cloned())))
    };
    (@noop $_mod:ident, $controller:ident, $_cfg:ident, $_theme:ident) => { let _ = $controller; };
    (@theme $mod:ident, $controller:ident, $config:ident, $theme_mode:ident) => {
        $controller.emit($mod::Input::Reconfigure {
            config: $mod::Config::from_raw(&$config.cloned()),
            theme_mode: $theme_mode,
        })
    };
    (@pop $mod:ident, $controller:ident) => {{
        $controller.emit($mod::Input::TogglePopover);
        true
    }};
    (@nopop $_mod:ident, $controller:ident) => {{
        let _ = $controller;
        false
    }};

    // Main arm: generates the enum + four uniform impl methods.
    // Each row: (VariantName, mod_ident, AppletPath, AppletTypeVariant, reconfigure_style, popover)
    //   reconfigure_style: std | noop | theme
    //   popover: pop (has Input::TogglePopover) | nopop (no popover)
    ( $( ($Variant:ident, $mod:ident, $Applet:path, $Type:ident, $reconfig:ident, $popover:ident) ),* $(,)? ) => {
        pub enum AppletController {
            $($Variant(Controller<$Applet>),)*
        }

        impl AppletController {
            pub fn applet_type(&self) -> AppletType {
                match self {
                    $(Self::$Variant(_) => AppletType::$Type,)*
                }
            }

            pub fn widget(&self) -> gtk::Widget {
                match self {
                    $(Self::$Variant(c) => c.widget().clone().upcast(),)*
                }
            }

            pub fn reconfigure(&self, config: Option<&AppletConfig>, theme_mode: ThemeMode) {
                match self {
                    $(Self::$Variant(controller) => {
                        define_applet_controller!(@$reconfig $mod, controller, config, theme_mode);
                    })*
                }
            }

            /// Toggles this applet's popover, if it has one. Returns `false` for
            /// applet types with no popover (e.g. pager, tray, workspace) so
            /// callers such as IPC command handling can report "unsupported"
            /// instead of silently doing nothing.
            pub fn toggle_popover(&self) -> bool {
                match self {
                    $(Self::$Variant(controller) => {
                        define_applet_controller!(@$popover $mod, controller)
                    })*
                }
            }
        }
    };
}

define_applet_controller! {
    (Audio,         audio,         audio::Applet,         Audio,         std,   pop),
    (Battery,       battery,       battery::Applet,       Battery,       std,   pop),
    (Brightness,    brightness,    brightness::Applet,    Brightness,    std,   pop),
    (Bluetooth,     bluetooth,     bluetooth::Applet,     Bluetooth,     std,   pop),
    (Display,       display,       display::Applet,       Display,       std,   pop),
    (Clipboard,     clipboard,     clipboard::Applet,     Clipboard,     std,   pop),
    (Clock,         clock,         clock::Applet,         Clock,         std,   pop),
    (Command,       command,       command::Applet,       Command,       std,   nopop),
    // Dynamic hosts N independent runtime connections, each with its own
    // popover; there is no single applet-level popover to address generically.
    (Dynamic,       dynamic,       dynamic::Applet,       Dynamic,       noop,  nopop),
    // Exec's popovers belong to its dynamically-hosted status items, not to
    // the applet's own top-level Input, so there is no single popover to
    // address generically here.
    (Exec,          exec,          exec::Applet,          Exec,          std,   nopop),
    (Idle,          idle,          idle::applet::Applet,  Idle,          noop,  pop),
    (Keyboard,      keyboard,      keyboard::Applet,      Keyboard,      std,   nopop),
    (Mpris,         mpris,         mpris::Applet,         Mpris,         std,   pop),
    (Network,       network,       network::Applet,       Network,       std,   pop),
    (NextEvent,     next_event,    next_event::Applet,    NextEvent,     std,   pop),
    (Notifications, notifications, notifications::Applet, Notifications, theme, pop),
    (Pager,         pager,         pager::Applet,         Pager,         std,   nopop),
    (Privacy,       privacy,       privacy::Applet,       Privacy,       std,   nopop),
    (Printing,      printing,      printing::Applet,      Printing,      std,   pop),
    (Removable,     removable,     removable::Applet,     Removable,     std,   pop),
    (Session,       session,       session::Applet,       Session,       theme, pop),
    (Tray,          tray,          tray::Applet,          Tray,          std,   nopop),
    (Weather,       weather,       weather::Applet,       Weather,       std,   pop),
    (Window,        window,        window::Applet,        Window,        std,   nopop),
    (Workspace,     workspace,     workspace::Applet,     Workspace,     std,   nopop),
}

pub fn create_applet(
    blueprint: AppletBlueprint,
    services: Services,
    monitor_connector: Option<&str>,
    dynamic_container: &gtk::Box,
    theme_mode: ThemeMode,
    ipc: &IpcEmitter,
) -> Option<AppletController> {
    match blueprint.applet_type {
        AppletType::Audio => Some(AppletController::Audio(
            audio::Applet::builder()
                .launch(audio::Init {
                    service: services.audio.clone(),
                    config: audio::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Battery => Some(AppletController::Battery(
            battery::Applet::builder()
                .launch(battery::Init {
                    service: services.battery.clone(),
                    power_service: services.power.clone(),
                    config: battery::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Bluetooth => Some(AppletController::Bluetooth(
            bluetooth::Applet::builder()
                .launch(bluetooth::Init {
                    service: services.bluetooth.clone(),
                    config: bluetooth::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Brightness => Some(AppletController::Brightness(
            brightness::Applet::builder()
                .launch(brightness::Init {
                    service: services.brightness.clone(),
                    compositor: services.compositor.clone(),
                    config: brightness::Config::from_raw(&blueprint.config),
                    panel_monitor: monitor_connector.map(str::to_owned),
                })
                .detach(),
        )),
        AppletType::Display => Some(AppletController::Display(
            display::Applet::builder()
                .launch(display::Init {
                    compositor: services.compositor.clone(),
                    config: display::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Clipboard => Some(AppletController::Clipboard(
            clipboard::Applet::builder()
                .launch(clipboard::Init {
                    service: services.clipboard.clone(),
                    config: clipboard::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Clock => Some(AppletController::Clock(
            clock::Applet::builder()
                .launch(clock::Init {
                    clock: services.clock.clone(),
                    calendar: services.calendar_events.clone(),
                    config: clock::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Command => {
            let config = command::Config::from_raw(&blueprint.config);
            if !command::Applet::can_launch(&config) {
                tracing::warn!(name = %blueprint.name, "command applet requires an icon or label");
                return None;
            }
            Some(AppletController::Command(
                command::Applet::builder()
                    .launch(command::Init {
                        name: blueprint.name,
                        config,
                    })
                    .detach(),
            ))
        }
        AppletType::Dynamic => Some(AppletController::Dynamic(
            dynamic::Applet::builder()
                .launch(dynamic::Init {
                    runtime_container: dynamic_container.clone(),
                })
                .detach(),
        )),
        AppletType::Exec => {
            let config = exec::Config::from_raw(&blueprint.config);
            if !exec::Applet::can_launch(&config) {
                tracing::warn!(name = %blueprint.name, "exec applet requires a non-empty command");
                return None;
            }
            Some(AppletController::Exec(
                exec::Applet::builder()
                    .launch(exec::Init {
                        name: blueprint.name,
                        config,
                        ipc: ipc.clone(),
                    })
                    .detach(),
            ))
        }
        AppletType::Idle => {
            let Some(ext) = SHELL_EXTENSIONS.get() else {
                tracing::warn!(
                    name = %blueprint.name,
                    "idle applet skipped: idle subsystem unavailable"
                );
                return None;
            };
            Some(AppletController::Idle(
                idle::applet::Applet::builder()
                    .launch(idle::applet::Init {
                        service: ext.idle_inhibitor.clone(),
                        wayland_health: ext.wayland_health.clone(),
                        own_unique_name: ext.own_unique_bus_name.clone(),
                    })
                    .detach(),
            ))
        }
        AppletType::Keyboard => Some(AppletController::Keyboard(
            keyboard::Applet::builder()
                .launch(keyboard::Init {
                    service: services.keyboard.clone(),
                    config: keyboard::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Network => Some(AppletController::Network(
            network::Applet::builder()
                .launch(network::Init {
                    service: services.network.clone(),
                    config: network::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Mpris => Some(AppletController::Mpris(
            mpris::Applet::builder()
                .launch(mpris::Init {
                    service: services.mpris.clone(),
                    config: mpris::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::NextEvent => Some(AppletController::NextEvent(
            next_event::Applet::builder()
                .launch(next_event::Init {
                    service: services.calendar_events.clone(),
                    config: next_event::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Notifications => Some(AppletController::Notifications(
            notifications::Applet::builder()
                .launch(notifications::Init {
                    service: services.notifications.clone(),
                    compositor: services.compositor.clone(),
                    config: notifications::Config::from_raw(&blueprint.config),
                    panel_monitor: monitor_connector.map(str::to_owned),
                    theme_mode,
                })
                .detach(),
        )),
        AppletType::Pager => Some(AppletController::Pager(
            pager::Applet::builder()
                .launch(pager::Init {
                    service: services.compositor.clone(),
                    config: pager::Config::from_raw(&blueprint.config),
                    panel_monitor: monitor_connector.map(str::to_owned),
                })
                .detach(),
        )),
        AppletType::Privacy => Some(AppletController::Privacy(
            privacy::Applet::builder()
                .launch(privacy::Init {
                    microphone: services.microphone.clone(),
                    webcam: services.webcam.clone(),
                    compositor: services.compositor.clone(),
                    geoclue: services.geoclue.clone(),
                    config: privacy::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Printing => Some(AppletController::Printing(
            printing::Applet::builder()
                .launch(printing::Init {
                    service: services.printing.clone(),
                    config: printing::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Removable => Some(AppletController::Removable(
            removable::Applet::builder()
                .launch(removable::Init {
                    service: services.storage.clone(),
                    config: removable::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Session => Some(AppletController::Session(
            session::Applet::builder()
                .launch(session::Init {
                    service: services.session.clone(),
                    config: session::Config::from_raw(&blueprint.config),
                    theme_mode,
                })
                .detach(),
        )),
        AppletType::Tray => Some(AppletController::Tray(
            tray::Applet::builder()
                .launch(tray::Init {
                    service: services.tray.clone(),
                    config: tray::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Weather => Some(AppletController::Weather(
            weather::Applet::builder()
                .launch(weather::Init {
                    service: services.weather.clone(),
                    config: weather::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Window => Some(AppletController::Window(
            window::Applet::builder()
                .launch(window::Init {
                    service: services.compositor.clone(),
                    config: window::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Workspace => Some(AppletController::Workspace(
            workspace::Applet::builder()
                .launch(workspace::Init {
                    service: services.compositor.clone(),
                    config: workspace::Config::from_raw(&blueprint.config),
                    panel_monitor: monitor_connector.map(str::to_owned),
                })
                .detach(),
        )),
    }
}

pub fn build_applets(
    section: PanelSection,
    configured_applets: &[String],
    container: &gtk::Box,
    dynamic_container: &gtk::Box,
    applet_configs: &HashMap<String, AppletConfig>,
    services: Services,
    monitor_connector: Option<&str>,
    theme_mode: ThemeMode,
    ipc: &IpcEmitter,
    panel_monitor: &str,
) -> HashMap<AppletKey, AppletController> {
    let mut applets = HashMap::new();
    let entries = collect_applets(section, configured_applets, applet_configs);
    for entry in entries {
        tracing::debug!(name = %entry.name, applet_type = ?entry.applet_type, "loading applet");

        if let Some(applet) = create_applet(
            entry.clone(),
            services.clone(),
            monitor_connector,
            dynamic_container,
            theme_mode,
            ipc,
        ) {
            let widget = applet.widget();
            widget.set_valign(gtk::Align::Center);
            container.append(&widget);
            emit_applet(
                ipc,
                "applet.added",
                panel_monitor,
                &entry.key,
                entry.applet_type,
            );
            applets.insert(entry.key, applet);
        }
    }

    applets
}

pub fn reconcile_applets(
    section: PanelSection,
    configured_applets: &[String],
    container: &gtk::Box,
    dynamic_container: &gtk::Box,
    current: &mut HashMap<AppletKey, AppletController>,
    previous_applet_configs: &HashMap<String, AppletConfig>,
    applet_configs: &HashMap<String, AppletConfig>,
    services: Services,
    monitor_connector: Option<&str>,
    theme_mode: ThemeMode,
    ipc: &IpcEmitter,
    panel_monitor: &str,
) {
    let current_types: HashMap<AppletKey, AppletType> = current
        .iter()
        .map(|(key, controller)| (key.clone(), controller.applet_type()))
        .collect();
    let plan = plan_reconcile_applets(
        section,
        configured_applets,
        &current_types,
        previous_applet_configs,
        applet_configs,
    );
    let mut remaining = std::mem::take(current);
    let mut next = HashMap::with_capacity(plan.ordered.len());
    let mut previous_widget: Option<gtk::Widget> = None;

    for planned in plan.ordered {
        let entry = planned.blueprint;
        let controller = match planned.action {
            PlannedAction::Reuse => remaining
                .remove(&entry.key)
                .expect("existing applet missing"),
            PlannedAction::Reconfigure => {
                let existing = remaining
                    .remove(&entry.key)
                    .expect("existing applet missing");
                existing.reconfigure(entry.config.as_ref(), theme_mode);
                existing
            }
            PlannedAction::Replace => {
                tracing::debug!(name = %entry.name, applet_type = ?entry.applet_type, "replacing applet");
                let existing = remaining
                    .remove(&entry.key)
                    .expect("existing applet missing");
                detach_widget(&existing.widget());
                let Some(created) = create_applet(
                    entry.clone(),
                    services.clone(),
                    monitor_connector,
                    dynamic_container,
                    theme_mode,
                    ipc,
                ) else {
                    continue;
                };
                created
            }
            PlannedAction::Create => {
                tracing::debug!(name = %entry.name, applet_type = ?entry.applet_type, "adding applet");
                let Some(created) = create_applet(
                    entry.clone(),
                    services.clone(),
                    monitor_connector,
                    dynamic_container,
                    theme_mode,
                    ipc,
                ) else {
                    continue;
                };
                created
            }
        };

        match planned.action {
            PlannedAction::Reconfigure => emit_applet(
                ipc,
                "applet.updated",
                panel_monitor,
                &entry.key,
                entry.applet_type,
            ),
            PlannedAction::Create => emit_applet(
                ipc,
                "applet.added",
                panel_monitor,
                &entry.key,
                entry.applet_type,
            ),
            PlannedAction::Replace => {
                if let Some(old) = current_types.get(&entry.key) {
                    emit_applet(ipc, "applet.removed", panel_monitor, &entry.key, *old);
                }
                emit_applet(
                    ipc,
                    "applet.added",
                    panel_monitor,
                    &entry.key,
                    entry.applet_type,
                );
            }
            PlannedAction::Reuse => {}
        }

        let widget = controller.widget();
        place_widget(container, &widget, previous_widget.as_ref());
        previous_widget = Some(widget);
        next.insert(entry.key, controller);
    }

    for key in plan.removals {
        if let Some(leftover) = remaining.remove(&key) {
            tracing::debug!(name = %key.name, "removing applet");
            detach_widget(&leftover.widget());
            if let Some(removed_type) = current_types.get(&key) {
                emit_applet(ipc, "applet.removed", panel_monitor, &key, *removed_type);
            }
        }
    }

    *current = next;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlannedAction {
    Reuse,
    Reconfigure,
    Replace,
    Create,
}

#[derive(Debug, Clone)]
struct PlannedApplet {
    blueprint: AppletBlueprint,
    action: PlannedAction,
}

#[derive(Debug, Clone)]
struct ReconcilePlan {
    ordered: Vec<PlannedApplet>,
    removals: Vec<AppletKey>,
}

fn plan_reconcile_applets(
    section: PanelSection,
    configured_applets: &[String],
    current_types: &HashMap<AppletKey, AppletType>,
    previous_applet_configs: &HashMap<String, AppletConfig>,
    applet_configs: &HashMap<String, AppletConfig>,
) -> ReconcilePlan {
    let entries = collect_applets(section, configured_applets, applet_configs);
    let mut remaining = current_types.clone();
    let mut ordered = Vec::with_capacity(entries.len());

    for entry in entries {
        let action = match remaining.remove(&entry.key) {
            Some(existing_type) if existing_type == entry.applet_type => {
                if previous_applet_configs.get(&entry.name) != applet_configs.get(&entry.name) {
                    PlannedAction::Reconfigure
                } else {
                    PlannedAction::Reuse
                }
            }
            Some(_) => PlannedAction::Replace,
            None => PlannedAction::Create,
        };

        ordered.push(PlannedApplet {
            blueprint: entry,
            action,
        });
    }

    ReconcilePlan {
        ordered,
        removals: remaining.into_keys().collect(),
    }
}

pub fn collect_applets(
    section: PanelSection,
    configured: &[String],
    applet_configs: &HashMap<String, AppletConfig>,
) -> Vec<AppletBlueprint> {
    let mut name_counts: HashMap<&str, usize> = HashMap::new();

    configured
        .iter()
        .filter_map(|name| {
            let occurrence = name_counts.entry(name.as_str()).or_insert(0);
            let resolved = resolve_applet(section.clone(), name, *occurrence, applet_configs);
            *occurrence += 1;
            resolved
        })
        .collect()
}

fn place_widget(container: &gtk::Box, widget: &gtk::Widget, sibling: Option<&gtk::Widget>) {
    widget.set_valign(gtk::Align::Center);
    match widget.parent() {
        Some(parent) if parent == container.clone().upcast::<gtk::Widget>() => {
            container.reorder_child_after(widget, sibling);
        }
        Some(_) => {
            detach_widget(widget);
            container.insert_child_after(widget, sibling);
        }
        None => {
            container.insert_child_after(widget, sibling);
        }
    }
}

fn detach_widget(widget: &gtk::Widget) {
    if let Some(parent_box) = widget.parent().and_downcast::<gtk::Box>() {
        parent_box.remove(widget);
    }
}

fn resolve_applet(
    section: PanelSection,
    name: &str,
    occurrence: usize,
    applet_configs: &HashMap<String, AppletConfig>,
) -> Option<AppletBlueprint> {
    let applet_config = applet_configs.get(name).cloned();
    let applet_type = applet_config
        .as_ref()
        .and_then(|config| config.extends)
        .or_else(|| AppletType::from_config_name(name));

    let Some(applet_type) = applet_type else {
        tracing::warn!(name, "unknown applet config, ignoring");
        return None;
    };

    let key = AppletKey {
        section,
        name: name.to_string(),
        occurrence,
    };

    Some(AppletBlueprint {
        key,
        name: name.to_string(),
        applet_type,
        config: applet_config,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_applets_uses_named_config_entry() {
        let mut applet_configs = HashMap::new();
        applet_configs.insert(
            "laptop".to_string(),
            AppletConfig {
                extends: Some(AppletType::Battery),
                settings: toml::Value::Table(toml::map::Map::new()),
            },
        );

        let entries = collect_applets(PanelSection::Right, &["laptop".into()], &applet_configs);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "laptop");
        assert_eq!(entries[0].applet_type, AppletType::Battery);
        assert!(entries[0].config.is_some());
    }

    #[test]
    fn collect_applets_uses_named_exec_package_entry() {
        let mut applet_configs = HashMap::new();
        applet_configs.insert(
            "sysinfo".to_string(),
            AppletConfig {
                extends: Some(AppletType::Exec),
                settings: toml::Value::Table(toml::map::Map::new()),
            },
        );

        let entries = collect_applets(PanelSection::Left, &["sysinfo".into()], &applet_configs);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "sysinfo");
        assert_eq!(entries[0].applet_type, AppletType::Exec);
        assert!(entries[0].config.is_some());
    }

    #[test]
    fn collect_applets_uses_named_command_package_entry() {
        let mut applet_configs = HashMap::new();
        applet_configs.insert(
            "launcher".to_string(),
            AppletConfig {
                extends: Some(AppletType::Command),
                settings: toml::Value::Table(toml::map::Map::new()),
            },
        );

        let entries = collect_applets(PanelSection::Left, &["launcher".into()], &applet_configs);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "launcher");
        assert_eq!(entries[0].applet_type, AppletType::Command);
        assert!(entries[0].config.is_some());
    }

    #[test]
    fn collect_applets_falls_back_to_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["battery".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "battery");
        assert_eq!(entries[0].applet_type, AppletType::Battery);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_falls_back_to_bluetooth_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["bluetooth".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "bluetooth");
        assert_eq!(entries[0].applet_type, AppletType::Bluetooth);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_falls_back_to_display_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["display".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "display");
        assert_eq!(entries[0].applet_type, AppletType::Display);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_falls_back_to_clipboard_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["clipboard".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "clipboard");
        assert_eq!(entries[0].applet_type, AppletType::Clipboard);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_falls_back_to_clock_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["clock".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "clock");
        assert_eq!(entries[0].applet_type, AppletType::Clock);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_resolves_dynamic_slot() {
        let entries = collect_applets(
            PanelSection::Right,
            &["__dynamic__".into()],
            &HashMap::new(),
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "__dynamic__");
        assert_eq!(entries[0].applet_type, AppletType::Dynamic);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_rejects_bare_command_name() {
        let entries = collect_applets(PanelSection::Left, &["command".into()], &HashMap::new());

        assert!(entries.is_empty());
    }

    #[test]
    fn collect_applets_rejects_bare_exec_name() {
        let entries = collect_applets(PanelSection::Left, &["exec".into()], &HashMap::new());

        assert!(entries.is_empty());
    }

    #[test]
    fn collect_applets_falls_back_to_network_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["network".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "network");
        assert_eq!(entries[0].applet_type, AppletType::Network);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_falls_back_to_mpris_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["mpris".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "mpris");
        assert_eq!(entries[0].applet_type, AppletType::Mpris);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_falls_back_to_notifications_builtin_name() {
        let entries = collect_applets(
            PanelSection::Left,
            &["notifications".into()],
            &HashMap::new(),
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "notifications");
        assert_eq!(entries[0].applet_type, AppletType::Notifications);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_falls_back_to_pager_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["pager".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "pager");
        assert_eq!(entries[0].applet_type, AppletType::Pager);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_falls_back_to_privacy_builtin_name() {
        let entries = collect_applets(PanelSection::Right, &["privacy".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "privacy");
        assert_eq!(entries[0].applet_type, AppletType::Privacy);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_falls_back_to_removable_builtin_name() {
        let entries = collect_applets(PanelSection::Right, &["removable".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "removable");
        assert_eq!(entries[0].applet_type, AppletType::Removable);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_falls_back_to_session_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["session".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "session");
        assert_eq!(entries[0].applet_type, AppletType::Session);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_falls_back_to_tray_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["tray".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "tray");
        assert_eq!(entries[0].applet_type, AppletType::Tray);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_uses_builtin_type_for_named_builtin_config_without_extends() {
        let mut applet_configs = HashMap::new();
        applet_configs.insert("battery".to_string(), AppletConfig::default());

        let entries = collect_applets(PanelSection::Left, &["battery".into()], &applet_configs);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "battery");
        assert_eq!(entries[0].applet_type, AppletType::Battery);
        assert!(entries[0].config.is_some());
        assert_eq!(entries[0].config.as_ref().unwrap().extends, None);
    }

    #[test]
    fn collect_applets_ignores_unknown_named_config_without_extends() {
        let mut applet_configs = HashMap::new();
        applet_configs.insert("custom_battery".to_string(), AppletConfig::default());

        let entries = collect_applets(
            PanelSection::Right,
            &["custom_battery".into()],
            &applet_configs,
        );

        assert!(entries.is_empty());
    }

    #[test]
    fn collect_applets_assigns_stable_occurrence_keys_for_duplicates() {
        let entries = collect_applets(
            PanelSection::Left,
            &[
                "battery".into(),
                "custom".into(),
                "battery".into(),
                "battery".into(),
            ],
            &HashMap::from([(
                "custom".into(),
                AppletConfig {
                    extends: Some(AppletType::Battery),
                    settings: toml::Value::Table(toml::map::Map::new()),
                },
            )]),
        );

        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].key.occurrence, 0);
        assert_eq!(entries[1].key.occurrence, 0);
        assert_eq!(entries[2].key.occurrence, 1);
        assert_eq!(entries[3].key.occurrence, 2);
    }

    #[test]
    fn collect_applets_keeps_duplicate_keys_stable_when_inserting_before_them() {
        let old = collect_applets(
            PanelSection::Left,
            &["battery".into(), "battery".into()],
            &HashMap::new(),
        );
        let new = collect_applets(
            PanelSection::Left,
            &["custom".into(), "battery".into(), "battery".into()],
            &HashMap::from([(
                "custom".into(),
                AppletConfig {
                    extends: Some(AppletType::Battery),
                    settings: toml::Value::Table(toml::map::Map::new()),
                },
            )]),
        );

        assert_eq!(old[0].key, new[1].key);
        assert_eq!(old[1].key, new[2].key);
    }

    #[test]
    fn plan_reconcile_reuses_duplicates_when_inserting_before_them() {
        let current_entries = collect_applets(
            PanelSection::Left,
            &["battery".into(), "battery".into()],
            &HashMap::new(),
        );
        let current_types = current_entries
            .iter()
            .map(|entry| (entry.key.clone(), entry.applet_type))
            .collect();

        let new_configs = HashMap::from([(
            "custom".into(),
            AppletConfig {
                extends: Some(AppletType::Battery),
                settings: toml::Value::Table(toml::map::Map::new()),
            },
        )]);
        let plan = plan_reconcile_applets(
            PanelSection::Left,
            &["custom".into(), "battery".into(), "battery".into()],
            &current_types,
            &HashMap::new(),
            &new_configs,
        );

        assert_eq!(plan.ordered.len(), 3);
        assert_eq!(plan.ordered[0].blueprint.name, "custom");
        assert_eq!(plan.ordered[0].action, PlannedAction::Create);
        assert_eq!(plan.ordered[1].blueprint.key, current_entries[0].key);
        assert_eq!(plan.ordered[1].action, PlannedAction::Reuse);
        assert_eq!(plan.ordered[2].blueprint.key, current_entries[1].key);
        assert_eq!(plan.ordered[2].action, PlannedAction::Reuse);
        assert!(plan.removals.is_empty());
    }

    #[test]
    fn plan_reconcile_marks_named_applet_for_reconfigure_on_config_change() {
        let current_entries = collect_applets(
            PanelSection::Left,
            &["battery".into()],
            &HashMap::from([("battery".into(), AppletConfig::default())]),
        );
        let current_types = current_entries
            .iter()
            .map(|entry| (entry.key.clone(), entry.applet_type))
            .collect();
        let previous_configs = HashMap::from([("battery".into(), AppletConfig::default())]);
        let next_configs = HashMap::from([(
            "battery".into(),
            AppletConfig {
                extends: None,
                settings: toml::Value::Table(toml::map::Map::from_iter([(
                    "show_icon".into(),
                    toml::Value::Boolean(false),
                )])),
            },
        )]);

        let plan = plan_reconcile_applets(
            PanelSection::Left,
            &["battery".into()],
            &current_types,
            &previous_configs,
            &next_configs,
        );

        assert_eq!(plan.ordered.len(), 1);
        assert_eq!(plan.ordered[0].blueprint.key, current_entries[0].key);
        assert_eq!(plan.ordered[0].action, PlannedAction::Reconfigure);
        assert!(plan.removals.is_empty());
    }

    #[test]
    fn plan_reconcile_removes_obsolete_applets() {
        let applet_configs = HashMap::from([(
            "custom".into(),
            AppletConfig {
                extends: Some(AppletType::Battery),
                settings: toml::Value::Table(toml::map::Map::new()),
            },
        )]);
        let current_entries = collect_applets(
            PanelSection::Left,
            &["battery".into(), "custom".into()],
            &applet_configs,
        );
        let current_types = current_entries
            .iter()
            .map(|entry| (entry.key.clone(), entry.applet_type))
            .collect();

        let plan = plan_reconcile_applets(
            PanelSection::Left,
            &["battery".into()],
            &current_types,
            &applet_configs,
            &HashMap::new(),
        );

        assert_eq!(plan.ordered.len(), 1);
        assert_eq!(plan.ordered[0].blueprint.key, current_entries[0].key);
        assert_eq!(plan.ordered[0].action, PlannedAction::Reuse);
        assert_eq!(plan.removals, vec![current_entries[1].key.clone()]);
    }
}
