use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{
    CommandError, CompositorCapabilities, CompositorHandle, OutputInfo, OutputLogical, OutputMode,
};
use glimpse_widgets::{
    Display as WidgetDisplay, DisplayLogical as WidgetDisplayLogical,
    DisplayMode as WidgetDisplayMode, DisplayPopover, IndicatorSpec,
};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{Applet, Ctx, Input, Report, report_failure};

use super::render;

fn themed(name: &str) -> gio::Icon {
    gio::ThemedIcon::new(name).upcast()
}

pub struct Display {
    outputs: Vec<OutputInfo>,
    capabilities: Option<CompositorCapabilities>,
    compositor: CompositorHandle,
    notifications: NotificationsProviderHandle,
    tooltip_format: Option<String>,
    footer: Option<(String, Vec<String>)>,
    spec: Vec<IndicatorSpec>,
    shown: glib::WeakRef<DisplayPopover>,
}

impl Applet for Display {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Display {} = &config.kind else {
            return;
        };
        self.tooltip_format = config.common.tooltip_format.clone();
        self.footer = config
            .common
            .settings()
            .map(|(label, command)| (label.to_owned(), command.to_vec()));
        self.refresh();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        if !matches!(input, Input::Woken) {
            return;
        }
        let state = self.compositor.snapshot();
        self.outputs = state
            .outputs
            .map(|outputs| outputs.outputs)
            .unwrap_or_default();
        self.capabilities = state.status.map(|status| status.capabilities);
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, _seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = DisplayPopover::new();

        shown.connect_enable_requested({
            let compositor = self.compositor.clone();
            let notifications = self.notifications.clone();
            move |_, connector, enabled| {
                let compositor = compositor.clone();
                let notifications = notifications.clone();
                let connector = connector.to_owned();
                relm4::spawn_local(async move {
                    if let Err(error) = compositor.set_output_enabled(connector, enabled).await {
                        report_display_failure(
                            &notifications,
                            "compositor.set_output_enabled",
                            gettext("Could not change that display"),
                            error,
                        )
                        .await;
                    }
                });
            }
        });

        shown.connect_blanked({
            let compositor = self.compositor.clone();
            let notifications = self.notifications.clone();
            move |_| {
                let compositor = compositor.clone();
                let notifications = notifications.clone();
                relm4::spawn_local(async move {
                    if let Err(error) = compositor.power_off_monitors().await {
                        report_display_failure(
                            &notifications,
                            "compositor.power_off_monitors",
                            gettext("Could not blank the screens"),
                            error,
                        )
                        .await;
                    }
                });
            }
        });

        if let Some((_, command)) = &self.footer {
            let command = command.clone();
            shown.connect_footer_activated(move |_| run(&command));
        }

        self.shown.set(Some(&shown));
        self.refresh();
        Some(Box::new(shown))
    }
}

async fn report_display_failure(
    notifications: &NotificationsProviderHandle,
    operation: &'static str,
    summary: String,
    error: CommandError,
) {
    let report = Report {
        notifications: notifications.clone(),
        app_name: gettext("Displays"),
        icon: render::SINGLE_ICON.to_owned(),
        summary,
    };
    report_failure(operation, report, wording(&error), error).await;
}

fn wording(error: &CommandError) -> Option<String> {
    Some(match error {
        CommandError::InvalidArgument(_) => gettext("That change was refused."),
        CommandError::Unavailable(_) => gettext("The compositor is unavailable."),
        CommandError::Unsupported(_) => gettext("That is not supported."),
        CommandError::LimitExceeded(_) => gettext("That could not be completed."),
        CommandError::Internal(_) => gettext("That did not work."),
    })
}

fn to_mode(mode: &OutputMode) -> WidgetDisplayMode {
    WidgetDisplayMode {
        width: mode.width as i32,
        height: mode.height as i32,
        refresh_mhz: mode.refresh_mhz as i32,
    }
}

fn to_logical(logical: &OutputLogical) -> WidgetDisplayLogical {
    WidgetDisplayLogical {
        x: logical.x,
        y: logical.y,
        scale: logical.scale,
    }
}

fn to_widget_display(output: &OutputInfo) -> WidgetDisplay {
    WidgetDisplay {
        connector: output.connector.clone(),
        label: output.label.clone().unwrap_or_default(),
        make: output.make.clone(),
        model: output.model.clone(),
        serial: output.serial.clone(),
        current_mode: output.current_mode.as_ref().map(to_mode),
        logical: output.logical.as_ref().map(to_logical),
        enabled: output.enabled,
    }
}

impl Display {
    pub fn start(compositor: CompositorHandle, notifications: NotificationsProviderHandle) -> Self {
        let state = compositor.snapshot();
        let outputs = state
            .outputs
            .map(|outputs| outputs.outputs)
            .unwrap_or_default();
        let capabilities = state.status.map(|status| status.capabilities);
        Self {
            outputs,
            capabilities,
            compositor,
            notifications,
            tooltip_format: None,
            footer: None,
            spec: Vec::new(),
            shown: glib::WeakRef::new(),
        }
    }

    fn refresh(&mut self) {
        self.spec = self.indicator().into_iter().collect();
        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn dress(&self, shown: &DisplayPopover) {
        let output_power = self.capabilities.is_some_and(|caps| caps.output_power);
        shown.set_output_power(output_power);
        let displays: Vec<WidgetDisplay> = self.outputs.iter().map(to_widget_display).collect();
        shown.set_displays(&displays);
        shown.set_footer(self.footer.as_ref().map(|(label, _)| label.as_str()));
    }

    fn indicator(&self) -> Option<IndicatorSpec> {
        let icon = render::chip(self.outputs.len())?;
        let default_tooltip = render::tooltip(&self.outputs);
        let tooltip = match self.tooltip_format.as_deref() {
            Some(format) => default_tooltip.as_deref().map(|name| {
                crate::applets::tokens::render(format, |token| match token {
                    "name" => Some(name),
                    _ => None,
                })
            }),
            None => default_tooltip,
        };
        Some(IndicatorSpec {
            icon: Some(themed(icon)),
            tooltip,
            ..Default::default()
        })
    }
}
