pub(crate) mod render;

use gettextrs::gettext;
use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_services::{SessionAction, SessionActionsHandle, SessionActionsState};
use glimpse_widgets::{
    HIBERNATE, IndicatorSpec, LOCK, LOG_OUT, POWER_OFF, REBOOT, SUSPEND, SessionPopover,
};
use gtk4::{gio, glib, prelude::*};

use crate::app::SessionDialog;
use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{Applet, Ctx, Input};

const ICON: &str = "system-shutdown-symbolic";

pub struct Session {
    actions: SessionActionsHandle,
    dialog: relm4::Sender<crate::app::AppInput>,
    state: SessionActionsState,
    footer: Option<(String, Vec<String>)>,
    tooltip_format: Option<String>,
    icon: gio::Icon,
    shown: glib::WeakRef<SessionPopover>,
}

impl Session {
    pub fn start(
        actions: SessionActionsHandle,
        dialog: relm4::Sender<crate::app::AppInput>,
    ) -> Self {
        Self {
            state: actions.snapshot(),
            actions,
            dialog,
            footer: None,
            tooltip_format: None,
            icon: gio::ThemedIcon::new(ICON).upcast(),
            shown: glib::WeakRef::new(),
        }
    }

    fn refresh(&mut self) {
        self.state = self.actions.snapshot();
        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn dress(&self, popover: &SessionPopover) {
        let (user, signed_in) = render::heading(&self.state);
        popover.set_heading(user, signed_in.as_deref());
        popover.set_action(LOCK, &render::always());
        popover.set_action(LOG_OUT, &render::always());
        let state = &self.state;
        for (name, action, capability) in [
            (SUSPEND, SessionAction::Suspend, state.suspend),
            (HIBERNATE, SessionAction::Hibernate, state.hibernate),
            (REBOOT, SessionAction::Reboot, state.reboot),
            (POWER_OFF, SessionAction::PowerOff, state.power_off),
        ] {
            popover.set_action(name, &render::action_state(state, action, capability));
        }
        popover.set_footer(self.footer.as_ref().map(|(label, _)| label.as_str()));
    }
}

impl Applet for Session {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Session {} = &config.kind else {
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
        if matches!(input, Input::Woken) {
            self.refresh();
        }
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        let tooltip = match self.tooltip_format.as_deref() {
            Some(format) => crate::applets::tokens::render(format, |token| match token {
                "user" => Some(self.state.user.as_deref().unwrap_or_default()),
                _ => None,
            }),
            None => gettext("Session"),
        };
        vec![IndicatorSpec {
            icon: Some(self.icon.clone()),
            tooltip: Some(tooltip),
            ..Default::default()
        }]
    }

    fn popover(&mut self, _seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = SessionPopover::new();
        shown.connect_action_requested({
            let actions = self.actions.clone();
            let dialog = self.dialog.clone();
            move |_, action| {
                let Some(action) = render::action_from(action) else {
                    return;
                };
                let state = actions.snapshot();
                if let Some((title, body, accept)) = render::confirm(&state, action.clone()) {
                    let _ = dialog.send(crate::app::AppInput::SessionConfirm(SessionDialog {
                        title,
                        body,
                        accept,
                        action,
                    }));
                    return;
                }
                let _ = dialog.send(crate::app::AppInput::SessionRun(action));
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
