use gtk4::glib;
use gtk4::prelude::*;

fn main() -> glib::ExitCode {
    glimpse_widgets::preview::run(ensure_types)
}

fn ensure_types() {
    if let Err(error) = glimpse_agents::dialog::register_resources() {
        eprintln!("preview: cannot register the agents resources: {error}");
    }
    glimpse_agents::dialog::AgentDialog::static_type();
}
