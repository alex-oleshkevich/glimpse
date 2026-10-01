use gtk4::gio::prelude::*;
use gtk4::{gio, glib};

pub struct ComponentActions {
    status: gio::SimpleAction,
}

impl ComponentActions {
    pub fn install(
        map: &impl IsA<gio::ActionMap>,
        version: &str,
        initial: Result<(), String>,
    ) -> Self {
        map.add_action(&read_only("version", &version.to_variant()));
        let status = read_only("config-status", &verdict_state(&initial));
        map.add_action(&status);
        Self { status }
    }

    pub fn report(&self, verdict: Result<(), String>) {
        let state = verdict_state(&verdict);
        if self.status.state().as_ref() != Some(&state) {
            self.status.set_state(&state);
        }
    }
}

pub fn read_only(name: &str, state: &glib::Variant) -> gio::SimpleAction {
    let action = gio::SimpleAction::new_stateful(name, None, state);
    action.connect_change_state(|_, _| {});
    action
}

fn verdict_state(verdict: &Result<(), String>) -> glib::Variant {
    match verdict {
        Ok(()) => (true, "").to_variant(),
        Err(message) => (false, message.as_str()).to_variant(),
    }
}

#[cfg(test)]
mod tests;
