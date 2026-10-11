mod render;

use glimpse_config::{Applet as AppletConfig, AppletKind, SystemMonitorChip as Chip};
use glimpse_services::{SystemMonitorHandle, SystemMonitorState};
use glimpse_widgets::{IndicatorSpec, SystemMonitorPopover};
use gtk4::glib;
use gtk4::prelude::Cast as _;

use crate::applet::footer::Footer;
use crate::applet::popover::Seat;
use crate::applet::{Applet, Ctx, Input};

pub struct SystemMonitor {
    handle: SystemMonitorHandle,
    state: SystemMonitorState,
    chips: Vec<Chip>,
    chip_format: String,
    warn_percent: u8,
    critical_percent: u8,
    tooltip_format: Option<String>,
    footer: Footer,
    spec: Vec<IndicatorSpec>,
    shown: glib::WeakRef<SystemMonitorPopover>,
}

impl Applet for SystemMonitor {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::SystemMonitor(cfg) = &config.kind else {
            return;
        };
        self.chips = cfg.chips.clone();
        self.chip_format = cfg.chip_format.clone();
        self.warn_percent = cfg.warn_percent;
        self.critical_percent = cfg.critical_percent;
        self.tooltip_format = config.common.tooltip_format.clone();
        self.footer.configure(&config.common);
        self.refresh();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => {
                self.state = self.handle.snapshot();
            }
            Input::Tick | Input::Pointer(_) => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, _seat: &Seat) -> Option<gtk4::Widget> {
        let shown = SystemMonitorPopover::new();

        let footer = self.footer.clone();
        shown.connect_footer_activated(move |_| footer.run());

        self.shown.set(Some(&shown));
        self.refresh();
        Some(shown.upcast())
    }
}

impl SystemMonitor {
    pub fn start(handle: SystemMonitorHandle) -> Self {
        let state = handle.snapshot();
        Self {
            handle,
            state,
            chips: vec![Chip::Cpu, Chip::Ram],
            chip_format: "{name} {value}".to_owned(),
            warn_percent: 85,
            critical_percent: 95,
            tooltip_format: None,
            footer: Footer::default(),
            spec: Vec::new(),
            shown: glib::WeakRef::new(),
        }
    }

    fn refresh(&mut self) {
        self.spec = self.tooltipped(render::chips(
            &self.state,
            &self.chips,
            &self.chip_format,
            self.warn_percent,
            self.critical_percent,
        ));
        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn tooltipped(&self, specs: Vec<IndicatorSpec>) -> Vec<IndicatorSpec> {
        match &self.tooltip_format {
            None => specs,
            Some(format) => specs
                .into_iter()
                .map(|spec| IndicatorSpec {
                    tooltip: Some(format.clone()),
                    ..spec
                })
                .collect(),
        }
    }

    fn dress(&self, shown: &SystemMonitorPopover) {
        shown.set_readings(&render::readings(
            &self.state,
            self.warn_percent,
            self.critical_percent,
        ));
        shown.set_footer(self.footer.label().as_deref());
    }
}
