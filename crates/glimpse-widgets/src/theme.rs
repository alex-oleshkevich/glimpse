use std::path::Path;

use gtk4::{CssProvider, gdk};

const THEME_PRIORITY: u32 = gtk4::STYLE_PROVIDER_PRIORITY_USER;
const DROPIN_PRIORITY: u32 = THEME_PRIORITY + 1;

pub struct Styles {
    theme: CssProvider,
    dropin: CssProvider,
}

impl Styles {
    pub fn install() -> Self {
        let theme = CssProvider::new();
        let dropin = CssProvider::new();
        report_parsing_errors(&theme);
        report_parsing_errors(&dropin);

        match gdk::Display::default() {
            Some(display) => {
                gtk4::style_context_add_provider_for_display(&display, &theme, THEME_PRIORITY);
                gtk4::style_context_add_provider_for_display(&display, &dropin, DROPIN_PRIORITY);
            }
            None => tracing::error!("no display; stylesheets will not be applied"),
        }

        Self { theme, dropin }
    }

    pub fn load(&self, theme: Option<&Path>, dropin: Option<&Path>) {
        load(&self.theme, theme);
        load(&self.dropin, dropin);
    }
}

fn load(provider: &CssProvider, path: Option<&Path>) {
    match path {
        Some(path) => {
            tracing::debug!(path = %path.display(), "loading stylesheet");
            provider.load_from_path(path);
        }
        None => {
            tracing::debug!("no stylesheet; clearing its provider");
            provider.load_from_string("");
        }
    }
}

fn report_parsing_errors(provider: &CssProvider) {
    provider.connect_parsing_error(|_, section, error| {
        tracing::error!(at = %section.to_str(), %error, "stylesheet");
    });
}
