use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, gio, prelude::*},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use glimpse_core::services::printing::{Command, PrintingHandle, State};

use crate::{
    panels::applets::AppletConfig, services::framework::ServiceCommand, utils::subscribe_service,
    widgets::panel_indicator::PanelIndicator,
};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DisplayMode {
    #[default]
    Auto,
    Always,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub display: DisplayMode,
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };
        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid printing applet config, using defaults");
                Self::default()
            }
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            display: DisplayMode::Auto,
        }
    }
}

use super::{
    format,
    popover::{Popover, PopoverInit, PopoverInput, PopoverOutput},
};

pub struct Applet {
    visible: bool,
    icon: &'static str,
    label: String,
    tooltip: String,
    config: Config,
    state: State,
    service: PrintingHandle,
    popover: Controller<Popover>,
    action_popover: gtk::PopoverMenu,
    subscription_cancel: CancellationToken,
}

#[derive(Debug)]
pub struct Init {
    pub service: PrintingHandle,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    ServiceStateChanged(State),
    Reconfigure(Config),
    TogglePopover,
    OpenContextMenu,
    Refresh,
    PopoverOutput(PopoverOutput),
}

#[relm4::component(pub)]
impl SimpleComponent for Applet {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        root = PanelIndicator {
            add_css_class: "printing",
            #[watch]
            set_visible: model.visible,
            #[watch]
            set_icon: Some(model.icon),
            #[watch]
            set_label: if model.label.is_empty() { None } else { Some(model.label.as_str()) },
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },
            connect_activated[sender] => move |_| {
                sender.input(Input::TogglePopover);
            },
            connect_secondary_clicked[sender] => move |_| {
                sender.input(Input::OpenContextMenu);
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let popover = Popover::builder()
            .launch(PopoverInit {
                parent: root.clone().upcast::<gtk::Box>(),
            })
            .forward(sender.input_sender(), Input::PopoverOutput);

        let state = init.service.snapshot();

        let actions = gio::SimpleActionGroup::new();
        root.insert_action_group("printing", Some(&actions));

        let refresh_action = gio::SimpleAction::new("refresh", None);
        refresh_action.connect_activate({
            let sender = sender.input_sender().clone();
            move |_, _| sender.emit(Input::Refresh)
        });
        actions.add_action(&refresh_action);

        let menu = gio::Menu::new();
        menu.append(Some("Refresh"), Some("printing.refresh"));
        let action_popover = gtk::PopoverMenu::from_model(Some(&menu));
        action_popover.set_parent(&root);
        action_popover.set_has_arrow(false);
        root.connect_destroy({
            let action_popover = action_popover.clone();
            move |_| action_popover.unparent()
        });

        let subscription_cancel = subscribe_service(
            init.service.subscribe(),
            sender.input_sender().clone(),
            Input::ServiceStateChanged,
        );
        let model = Applet {
            visible: applet_visible(&state, &init.config),
            icon: applet_icon(&state),
            label: format::label(&state),
            tooltip: format::tooltip(&state),
            config: init.config,
            state,
            service: init.service,
            popover,
            action_popover,
            subscription_cancel,
        };

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::ServiceStateChanged(state) => {
                self.visible = applet_visible(&state, &self.config);
                self.icon = applet_icon(&state);
                self.label = format::label(&state);
                self.tooltip = format::tooltip(&state);
                self.popover.emit(PopoverInput::UpdateState(state.clone()));
                self.state = state;
            }
            Input::Reconfigure(config) => {
                self.visible = applet_visible(&self.state, &config);
                self.config = config;
            }
            Input::TogglePopover => {
                // Pre-sync with cached state before toggling, then kick a
                // Refresh so the service can push anything newer — matches
                // brightness/audio/removable/clipboard's contract.
                self.popover
                    .emit(PopoverInput::UpdateState(self.state.clone()));
                self.send_command(Command::Refresh);
                self.popover.emit(PopoverInput::Toggle);
            }
            Input::OpenContextMenu => {
                self.action_popover.popup();
            }
            Input::Refresh => {
                self.send_command(Command::Refresh);
            }
            Input::PopoverOutput(PopoverOutput::Command(cmd)) => {
                self.send_command(cmd);
            }
            Input::PopoverOutput(PopoverOutput::OpenQueue(printer_name)) => {
                open_queue(&printer_name);
            }
        }
    }
}

impl Applet {
    fn send_command(&self, command: Command) {
        let service = self.service.clone();
        relm4::spawn(async move {
            if let Err(e) = service.send(ServiceCommand::Command(command)).await {
                tracing::warn!(%e, "failed to send printing command");
            }
        });
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.subscription_cancel.cancel();
    }
}

fn applet_visible(state: &State, config: &Config) -> bool {
    if state.printers.is_empty() {
        return false;
    }
    match config.display {
        DisplayMode::Always => true,
        DisplayMode::Auto => !state.jobs.is_empty() || format::has_errors(state),
    }
}

fn applet_icon(state: &State) -> &'static str {
    if format::has_errors(state) {
        "printer-error-symbolic"
    } else {
        "printer-symbolic"
    }
}

fn queue_url(printer_name: &str) -> String {
    let escaped_name = relm4::gtk::glib::Uri::escape_string(printer_name, None, false);
    format!("http://localhost:631/printers/{escaped_name}")
}

fn open_queue(printer_name: &str) {
    let url = queue_url(printer_name);
    gtk::UriLauncher::new(&url).launch(
        None::<&gtk::Window>,
        None::<&gio::Cancellable>,
        move |result| {
            if let Err(error) = result {
                tracing::warn!(%error, %url, "failed to open printer queue");
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::printing::{JobState, PrintJob};

    #[test]
    fn queue_url_percent_encodes_printer_name() {
        assert_eq!(
            queue_url("Office Printer #2"),
            "http://localhost:631/printers/Office%20Printer%20%232"
        );
    }

    fn make_job(id: u32) -> PrintJob {
        PrintJob {
            id,
            name: "Test.pdf".into(),
            printer_name: "Printer".into(),
            state: JobState::Processing,
            pages_completed: None,
            pages_total: None,
        }
    }

    #[test]
    fn applet_hidden_when_no_jobs_and_no_errors() {
        let state = State::default();
        assert!(!applet_visible(&state, &Config::default()));
    }

    #[test]
    fn applet_visible_with_label_when_jobs_present() {
        use glimpse_core::services::printing::{Printer, PrinterState};
        let state = State {
            available: true,
            jobs: vec![make_job(1)],
            printers: vec![Printer {
                name: "Printer".into(),
                make_model: "Test".into(),
                state: PrinterState::Processing,
                state_reasons: vec![],
                job_count: 1,
                markers: vec![],
            }],
        };
        assert!(applet_visible(&state, &Config::default()));
        assert_eq!(format::label(&state), "1");
    }

    #[test]
    fn applet_hidden_when_no_printers() {
        let state = State {
            available: true,
            jobs: vec![make_job(1)],
            printers: vec![],
        };
        assert!(!applet_visible(&state, &Config::default()));
        assert!(!applet_visible(
            &state,
            &Config {
                display: DisplayMode::Always
            }
        ));
    }

    #[test]
    fn applet_visible_and_error_icon_when_printer_stopped() {
        use glimpse_core::services::printing::Printer;
        let state = State {
            available: true,
            jobs: vec![],
            printers: vec![Printer {
                name: "Printer".into(),
                make_model: "Test".into(),
                state: glimpse_core::services::printing::PrinterState::Stopped,
                state_reasons: vec!["paper-jam".into()],
                job_count: 0,
                markers: vec![],
            }],
        };
        assert!(applet_visible(&state, &Config::default()));
        assert_eq!(applet_icon(&state), "printer-error-symbolic");
    }
}
