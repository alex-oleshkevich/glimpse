use chrono::{Duration, Local};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, gio, glib, prelude::*},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{
    panels::applets::AppletConfig,
    services::calendar_events::{CalendarEventsHandle, State},
    utils::subscribe_service,
    widgets::panel_indicator::PanelIndicator,
};

use super::{
    format::{self, NextEvent},
    popover::{Popover, PopoverInit, PopoverInput, PopoverOutput},
};

const DEFAULT_THRESHOLD_MINUTES: u32 = 30;
const PANEL_LABEL_MAX_WIDTH_CHARS: i32 = 48;
const TICK_INTERVAL_SECS: u32 = 60;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    #[serde(alias = "label")]
    pub label_format: String,
    #[serde(alias = "tooltip")]
    pub tooltip_format: String,
    pub threshold_minutes: u32,
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid next_event applet config, using defaults");
                Self::default()
            }
        }
    }

    fn normalized_threshold(&self) -> Duration {
        Duration::minutes(self.threshold_minutes.max(1) as i64)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            label_format: format::DEFAULT_LABEL_FORMAT.into(),
            tooltip_format: format::DEFAULT_TOOLTIP_FORMAT.into(),
            threshold_minutes: DEFAULT_THRESHOLD_MINUTES,
        }
    }
}

pub struct Applet {
    config: Config,
    label: String,
    label_markup: String,
    tooltip: String,
    hidden: bool,
    state: State,
    popover: Controller<Popover>,
    subscription_cancel: CancellationToken,
    tick_source: Option<glib::SourceId>,
}

#[derive(Debug)]
pub struct Init {
    pub service: CalendarEventsHandle,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    ServiceStateChanged(State),
    Reconfigure(Config),
    Tick,
    TogglePopover,
    PopoverOutput(PopoverOutput),
}

#[relm4::component(pub)]
impl SimpleComponent for Applet {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        root = PanelIndicator {
            set_label_max_width_chars: PANEL_LABEL_MAX_WIDTH_CHARS,
            set_label_ellipsize: gtk::pango::EllipsizeMode::End,
            set_label_single_line_mode: true,
            set_label_xalign: 0.0,
            #[watch]
            set_visible: !model.hidden,
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },
            #[watch]
            set_label_markup: if model.label_markup.is_empty() { None } else { Some(model.label_markup.as_str()) },
            connect_activated[sender] => move |_| {
                sender.input(Input::TogglePopover);
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
        let subscription_cancel = subscribe_service(
            init.service.subscribe(),
            sender.input_sender().clone(),
            Input::ServiceStateChanged,
        );
        let mut model = Applet {
            config: init.config,
            label: String::new(),
            label_markup: String::new(),
            tooltip: String::new(),
            hidden: true,
            state,
            popover,
            subscription_cancel,
            tick_source: None,
        };
        model.recompute();

        model.tick_source = Some(start_tick_timer(sender.clone()));

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::ServiceStateChanged(state) => {
                self.state = state;
                self.recompute();
            }
            Input::Reconfigure(config) => {
                self.config = config;
                self.recompute();
            }
            Input::Tick => self.recompute(),
            Input::TogglePopover => self.popover.emit(PopoverInput::Toggle),
            Input::PopoverOutput(PopoverOutput::Opened | PopoverOutput::Closed) => {}
            Input::PopoverOutput(PopoverOutput::OpenUri(uri)) => open_uri(&uri),
        }
    }
}

impl Applet {
    fn recompute(&mut self) {
        let now = Local::now();
        let threshold = self.config.normalized_threshold();
        match format::next_event(&self.state, threshold, now) {
            Some(event) => self.apply_event(&event, now),
            None => self.clear(now),
        }
    }

    fn apply_event(&mut self, event: &NextEvent, now: chrono::DateTime<Local>) {
        self.label = format::label(&self.config.label_format, event, now);
        self.label_markup = format::label_markup(&self.config.label_format, event, now);
        self.tooltip = format::tooltip(&self.config.tooltip_format, event, now);
        // An event was found (that's why apply_event was called at all), so
        // it's visible regardless of the rendered label text — an empty
        // label_format (icon-only) must not hide the applet.
        self.hidden = false;
        self.popover.emit(PopoverInput::Update {
            event: Some(event.clone()),
            now,
        });
    }

    fn clear(&mut self, now: chrono::DateTime<Local>) {
        self.label.clear();
        self.label_markup.clear();
        self.tooltip.clear();
        self.hidden = true;
        self.popover.emit(PopoverInput::Update { event: None, now });
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.subscription_cancel.cancel();
        if let Some(source) = self.tick_source.take() {
            source.remove();
        }
    }
}

fn start_tick_timer(sender: ComponentSender<Applet>) -> glib::SourceId {
    let input = sender.input_sender().clone();
    glib::timeout_add_seconds_local(TICK_INTERVAL_SECS, move || {
        if input.send(Input::Tick).is_err() {
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    })
}

fn open_uri(uri: &str) {
    gtk::UriLauncher::new(uri).launch(None::<&gtk::Window>, None::<&gio::Cancellable>, |_| {});
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_clamps_threshold_to_at_least_one_minute() {
        let cfg = Config {
            threshold_minutes: 0,
            ..Config::default()
        };
        assert_eq!(cfg.normalized_threshold(), Duration::minutes(1));
    }

    #[test]
    fn config_preserves_large_threshold() {
        let cfg = Config {
            threshold_minutes: 90,
            ..Config::default()
        };
        assert_eq!(cfg.normalized_threshold(), Duration::minutes(90));
    }
}
