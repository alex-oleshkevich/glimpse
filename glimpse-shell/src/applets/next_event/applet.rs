use chrono::{Duration, Local};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, glib, prelude::*},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{
    panels::applets::AppletConfig,
    services::calendar_events::{CalendarEventsHandle, State},
    widgets::panel_indicator::PanelIndicator,
};

use super::format::{self, NextEvent};

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
    tooltip: String,
    hidden: bool,
    state: State,
    service: CalendarEventsHandle,
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
            set_label: if model.label.is_empty() { None } else { Some(model.label.as_str()) },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let state = init.service.snapshot();
        let mut model = Applet {
            config: init.config,
            label: String::new(),
            tooltip: String::new(),
            hidden: true,
            state,
            service: init.service,
            subscription_cancel: CancellationToken::new(),
            tick_source: None,
        };
        model.recompute();

        spawn_subscription(
            &model.service,
            model.subscription_cancel.clone(),
            sender.clone(),
        );
        model.tick_source = Some(start_tick_timer(sender));

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
        }
    }
}

impl Applet {
    fn recompute(&mut self) {
        let now = Local::now();
        let threshold = self.config.normalized_threshold();
        match format::next_event(&self.state, threshold, now) {
            Some(event) => self.apply_event(&event, now),
            None => self.clear(),
        }
    }

    fn apply_event(&mut self, event: &NextEvent, now: chrono::DateTime<Local>) {
        self.label = format::label(&self.config.label_format, event, now);
        self.tooltip = format::tooltip(&self.config.tooltip_format, event, now);
        self.hidden = self.label.is_empty();
    }

    fn clear(&mut self) {
        self.label.clear();
        self.tooltip.clear();
        self.hidden = true;
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

fn spawn_subscription(
    service: &CalendarEventsHandle,
    cancel: CancellationToken,
    sender: ComponentSender<Applet>,
) {
    let service = service.clone();
    let sender = sender.input_sender().clone();
    relm4::spawn(async move {
        // init() already seeded state via service.snapshot(); subscribe just
        // for subsequent changes.
        let mut sub = service.subscribe();
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                changed = sub.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    if sender
                        .send(Input::ServiceStateChanged(sub.borrow().clone()))
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    });
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
