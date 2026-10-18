use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind, CommandAppletConfig};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_widgets::IndicatorSpec;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::applet::popover::launch;
use crate::applet::{Applet, Button, Ctx, Direction, Input, Pointer, Report, report_failure};

const FALLBACK_ICON: &str = "system-run-symbolic";
const REPORT_QUIET: Duration = Duration::from_secs(5);

pub struct Command {
    notifications: NotificationsProviderHandle,
    config: CommandAppletConfig,
    icon: Option<gio::Icon>,
    label: Option<String>,
    tooltip: Option<String>,
    report_icon: String,
    reported: Rc<RefCell<Option<(String, Instant)>>>,
}

impl Applet for Command {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Command(settings) = &config.kind else {
            return;
        };
        self.icon = settings.icon.as_deref().and_then(icon);
        self.label = settings.label.clone().filter(|label| !label.is_empty());
        self.tooltip = settings
            .tooltip_format
            .clone()
            .filter(|tooltip| !tooltip.is_empty());
        self.report_icon = settings
            .icon
            .clone()
            .filter(|icon| !icon.is_empty() && !icon.starts_with('/'))
            .unwrap_or_else(|| FALLBACK_ICON.to_owned());
        self.config = (**settings).clone();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        let Input::Pointer(pointer) = input else {
            return;
        };
        let command = argv(&self.config, *pointer).to_vec();
        self.run(command);
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        if self.icon.is_none() && self.label.is_none() {
            return Vec::new();
        }
        vec![IndicatorSpec {
            icon: self.icon.clone(),
            label: self.label.clone(),
            tooltip: self.tooltip.clone(),
            ..Default::default()
        }]
    }
}

impl Command {
    pub fn start(notifications: NotificationsProviderHandle) -> Self {
        Self {
            notifications,
            config: CommandAppletConfig::default(),
            icon: None,
            label: None,
            tooltip: None,
            report_icon: FALLBACK_ICON.to_owned(),
            reported: Rc::default(),
        }
    }

    fn run(&mut self, command: Vec<String>) {
        let env = self.config.env.clone();
        let inherit = self.config.inherit_env;
        let program = command.first().cloned().unwrap_or_default();
        let reported = self.reported.clone();
        let report = Report {
            notifications: self.notifications.clone(),
            app_name: self.label.clone().unwrap_or_else(|| gettext("Command")),
            icon: self.report_icon.clone(),
            summary: gettext("Could not run a command"),
        };
        let body = gettext("{program} could not be started.").replace("{program}", &program);
        relm4::spawn_local(async move {
            let Err(error) = launch(&command, inherit, &env).await else {
                return;
            };
            let now = Instant::now();
            if quiet(reported.borrow().as_ref(), &program, now) {
                tracing::debug!(program, %error, "command did not start again");
                return;
            }
            *reported.borrow_mut() = Some((program, now));
            report_failure("command.run", report, Some(body), error).await;
        });
    }
}

fn argv(config: &CommandAppletConfig, pointer: Pointer) -> &[String] {
    match pointer {
        Pointer::Press(Button::Left) => &config.on_click,
        Pointer::Press(Button::Middle) => &config.on_middle_click,
        Pointer::Press(Button::Right | Button::Other(_)) => &[],
        Pointer::Scroll(Direction::Up) => &config.on_scroll_up,
        Pointer::Scroll(Direction::Down) => &config.on_scroll_down,
        Pointer::Scroll(Direction::Left) => &config.on_scroll_left,
        Pointer::Scroll(Direction::Right) => &config.on_scroll_right,
    }
}

fn quiet(reported: Option<&(String, Instant)>, program: &str, now: Instant) -> bool {
    reported.is_some_and(|(last, at)| last == program && now.duration_since(*at) < REPORT_QUIET)
}

fn icon(name: &str) -> Option<gio::Icon> {
    if name.is_empty() {
        return None;
    }
    if !name.starts_with('/') {
        return Some(gio::ThemedIcon::new(name).upcast());
    }
    if !Path::new(name).is_file() {
        tracing::warn!(
            path = name,
            "command icon does not exist, showing the default"
        );
        return Some(gio::ThemedIcon::new(FALLBACK_ICON).upcast());
    }
    Some(gio::FileIcon::new(&gio::File::for_path(name)).upcast())
}

#[cfg(test)]
mod tests;
