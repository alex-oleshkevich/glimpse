use std::cell::Cell;
use std::rc::Rc;

use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind, ColorFormat};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{ColorPickerHandle, ColorPickerState, CommandError};
use glimpse_widgets::{ColorPickerPopover, IndicatorSpec, Swatch, rgba};
use gtk4::glib;
use gtk4::prelude::*;

use super::render;
use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{Applet, Button, Ctx, Input, Opener, Pointer, Report, spawn_reported};

const PICKING: &str = "color-picker--picking";

pub struct ColorPicker {
    service: ColorPickerHandle,
    notifications: NotificationsProviderHandle,
    state: ColorPickerState,
    tooltip_format: Option<String>,
    footer: Option<(String, Vec<String>)>,
    shown: glib::WeakRef<ColorPickerPopover>,
    spec: Vec<IndicatorSpec>,
    format: Rc<Cell<ColorFormat>>,
    swatch: Swatch,
    icon: gtk4::gio::Icon,
}

impl ColorPicker {
    pub fn start(service: ColorPickerHandle, notifications: NotificationsProviderHandle) -> Self {
        let state = service.snapshot();
        let mut applet = Self {
            format: Rc::new(Cell::new(state.format)),
            state,
            service,
            notifications,
            tooltip_format: None,
            footer: None,
            shown: glib::WeakRef::new(),
            spec: Vec::new(),
            swatch: Swatch::default(),
            icon: gtk4::gio::ThemedIcon::new(render::ICON).upcast(),
        };
        applet.refresh();
        applet
    }

    fn refresh(&mut self) {
        self.format.set(self.state.format);
        let latest = self.state.colors.first();
        if let Some(color) = latest {
            self.swatch.set_color(Some(&rgba(color.rgb)));
        }
        self.spec = vec![IndicatorSpec {
            icon: latest.is_none().then(|| self.icon.clone()),
            extension: latest.map(|_| self.swatch.clone().upcast()),
            tooltip: Some(render::tooltip(
                latest,
                self.state.format,
                self.tooltip_format.as_deref(),
            )),
            class: self.state.picking.then(|| PICKING.to_owned()),
            ..Default::default()
        }];
        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn dress(&self, shown: &ColorPickerPopover) {
        let format = self.state.format;
        let latest = self.state.colors.first().map(|color| color.rgb);
        let title = latest.map(|color| format.render(color));
        shown.set_latest(title.as_deref().zip(latest.map(rgba)));
        shown.set_shades(&render::shades(&self.state.colors, format));
        shown.set_footer(self.footer.as_ref().map(|(label, _)| label.as_str()));
    }

    fn report(&self, summary: String) -> Report {
        Report {
            notifications: self.notifications.clone(),
            app_name: gettext("Color picker"),
            icon: render::ICON.to_owned(),
            summary,
        }
    }
}

fn wording(error: &CommandError) -> Option<String> {
    match error {
        CommandError::LimitExceeded(_) => None,
        CommandError::InvalidArgument(_) => {
            Some(gettext("That color is no longer in the palette."))
        }
        CommandError::Unavailable(_) => Some(gettext(
            "The picker could not start, capture the screen or reach the clipboard.",
        )),
        CommandError::Unsupported(_) | CommandError::Internal(_) => {
            Some(gettext("That did not work."))
        }
    }
}

fn pick(service: &ColorPickerHandle, report: Report) {
    let service = service.clone();
    spawn_reported("color_picker.pick", report, wording, async move {
        service.pick().await
    });
}

fn copy(service: &ColorPickerHandle, report: Report, opener: Opener, id: u64, format: ColorFormat) {
    let Ok(id) = u32::try_from(id) else {
        return;
    };
    let service = service.clone();
    spawn_reported("color_picker.copy", report, wording, async move {
        service
            .copy(id, format)
            .await
            .inspect(|_| opener.acknowledge())
    });
}

impl Applet for ColorPicker {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::ColorPicker {} = &config.kind else {
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
        match input {
            Input::Woken => self.state = self.service.snapshot(),
            Input::Pointer(Pointer::Press(Button::Right)) => {
                if !self.state.picking {
                    pick(
                        &self.service,
                        self.report(gettext("Could not pick a color")),
                    );
                }
                return;
            }
            Input::Tick | Input::Pointer(_) => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = ColorPickerPopover::new();
        let opener = seat.opener();

        shown.connect_activated({
            let service = self.service.clone();
            let report = self.report(gettext("Could not copy that color"));
            let format = Rc::clone(&self.format);
            let opener = opener.clone();
            move |_, id| {
                copy(&service, report.clone(), opener.clone(), id, format.get());
                opener.close_popover();
            }
        });
        shown.connect_copied({
            let service = self.service.clone();
            let report = self.report(gettext("Could not copy that color"));
            let opener = opener.clone();
            move |_, id, key| {
                if let Some(format) = ColorFormat::parse(key) {
                    copy(&service, report.clone(), opener.clone(), id, format);
                }
                opener.close_popover();
            }
        });
        shown.connect_pick_requested({
            let service = self.service.clone();
            let report = self.report(gettext("Could not pick a color"));
            let opener = opener.clone();
            move |_| {
                opener.close_popover();
                if !service.snapshot().picking {
                    pick(&service, report.clone());
                }
            }
        });
        if let Some((_, command)) = &self.footer {
            let command = command.clone();
            shown.connect_footer_activated(move |_| run(&command));
        }

        self.shown.set(Some(&shown));
        self.dress(&shown);
        Some(Box::new(shown))
    }
}
