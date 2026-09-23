use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind, CommandAppletConfig};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_widgets::IndicatorSpec;
use gtk4::prelude::*;
use std::path::Path;
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
    reported: Option<(String, Instant)>,
}

impl Applet for Command {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Command(settings) = &config.kind else {
            return;
        };
        self.icon = settings.icon.as_deref().and_then(icon);
        self.label = settings.label.clone().filter(|label| !label.is_empty());
        self.tooltip = config
            .common
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
        self.run(&command);
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
            reported: None,
        }
    }

    fn run(&mut self, command: &[String]) {
        let Err(error) = launch(command) else {
            return;
        };
        let program = command.first().cloned().unwrap_or_default();
        let now = Instant::now();
        if quiet(self.reported.as_ref(), &program, now) {
            tracing::debug!(program, %error, "command did not start again");
            return;
        }
        self.reported = Some((program.clone(), now));
        let report = Report {
            notifications: self.notifications.clone(),
            app_name: self.label.clone().unwrap_or_else(|| gettext("Command")),
            icon: self.report_icon.clone(),
            summary: gettext("Could not run a command"),
        };
        let body = gettext("{program} could not be started.").replace("{program}", &program);
        relm4::spawn_local(report_failure("command.run", report, Some(body), error));
    }
}

fn argv(config: &CommandAppletConfig, pointer: Pointer) -> &[String] {
    match pointer {
        Pointer::Press(Button::Left) => &config.on_click,
        Pointer::Press(Button::Middle) => &config.on_middle_click,
        Pointer::Press(Button::Right) => &config.on_right_click,
        Pointer::Press(Button::Other(_)) => &[],
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
mod tests {
    use super::*;

    fn argv_of(program: &str) -> Vec<String> {
        vec![program.to_owned()]
    }

    #[test]
    fn every_gesture_runs_its_own_program() {
        let config = CommandAppletConfig {
            on_click: argv_of("left"),
            on_middle_click: argv_of("middle"),
            on_right_click: argv_of("right"),
            on_scroll_up: argv_of("up"),
            on_scroll_down: argv_of("down"),
            on_scroll_left: argv_of("west"),
            on_scroll_right: argv_of("east"),
            ..Default::default()
        };
        let cases = [
            (Pointer::Press(Button::Left), "left"),
            (Pointer::Press(Button::Middle), "middle"),
            (Pointer::Press(Button::Right), "right"),
            (Pointer::Scroll(Direction::Up), "up"),
            (Pointer::Scroll(Direction::Down), "down"),
            (Pointer::Scroll(Direction::Left), "west"),
            (Pointer::Scroll(Direction::Right), "east"),
        ];
        for (pointer, program) in cases {
            assert_eq!(argv(&config, pointer), [program], "{pointer:?}");
        }
        assert!(argv(&config, Pointer::Press(Button::Other(8))).is_empty());
    }

    #[test]
    fn an_unconfigured_gesture_runs_nothing() {
        let config = CommandAppletConfig::default();
        assert!(argv(&config, Pointer::Press(Button::Left)).is_empty());
        assert!(launch(argv(&config, Pointer::Press(Button::Left))).is_ok());
    }

    #[test]
    fn an_icon_is_a_theme_name_unless_it_is_a_path() {
        assert!(icon("").is_none());
        assert!(icon("camera-photo-symbolic").is_some_and(|icon| icon.is::<gio::ThemedIcon>()));
        let present = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        assert!(icon(present).is_some_and(|icon| icon.is::<gio::FileIcon>()));
    }

    #[test]
    fn a_missing_image_falls_back_to_the_default_icon() {
        let missing = icon("/nonexistent/glimpse/shot.png")
            .and_then(|icon| icon.downcast::<gio::ThemedIcon>().ok())
            .expect("a themed fallback");
        assert_eq!(
            missing.names().first().map(|name| name.as_str()),
            Some(FALLBACK_ICON)
        );
    }

    #[test]
    fn a_repeated_failure_is_reported_once_per_quiet_period() {
        let now = Instant::now();
        let reported = ("pamixer".to_owned(), now);
        assert!(!quiet(None, "pamixer", now));
        assert!(quiet(
            Some(&reported),
            "pamixer",
            now + Duration::from_secs(1)
        ));
        assert!(!quiet(Some(&reported), "grim", now));
        assert!(!quiet(Some(&reported), "pamixer", now + REPORT_QUIET));
    }

    #[tokio::test]
    async fn a_chip_with_neither_icon_nor_label_takes_no_room() {
        let services = crate::services::PanelServices::start_with_buses(
            &glimpse_config::Config::default(),
            glimpse_dbus::Buses::unavailable("no bus in tests"),
        );
        let mut command = Command::start(services.notifications());
        assert!(command.indicators().is_empty());

        command.label = Some("Shot".to_owned());
        let specs = command.indicators();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].label.as_deref(), Some("Shot"));
        assert!(specs[0].icon.is_none());

        services.shutdown().await;
    }

    #[test]
    fn a_launch_that_cannot_start_is_an_error() {
        assert!(launch(&argv_of("glimpse-no-such-program")).is_err());
    }
}
