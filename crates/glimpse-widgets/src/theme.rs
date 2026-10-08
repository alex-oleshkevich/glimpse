use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::{
    CssProvider, gdk, glib,
    prelude::{ObjectExt, WidgetExt},
};

pub const BUILTIN: &str = include_str!("../styles/glimpse.css");

const BUILTIN_PRIORITY: u32 = gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION;
const THEME_PRIORITY: u32 = gtk4::STYLE_PROVIDER_PRIORITY_USER;
const THEME_DARK_PRIORITY: u32 = THEME_PRIORITY + 1;
const DROPIN_PRIORITY: u32 = THEME_PRIORITY + 2;
const DROPIN_DARK_PRIORITY: u32 = THEME_PRIORITY + 3;
const SPEED_PRIORITY: u32 = THEME_PRIORITY + 4;

const DURATION_MS: u32 = 150;

thread_local! {
    static SPEED: Cell<f64> = const { Cell::new(1.0) };
}

pub fn duration_ms() -> u32 {
    animation_ms(DURATION_MS)
}

pub(crate) fn animation_ms(base: u32) -> u32 {
    scaled(base, SPEED.get())
}

fn scaled(base: u32, speed: f64) -> u32 {
    if speed <= 0.0 {
        return 0;
    }
    (f64::from(base) / speed).round() as u32
}

fn speed_sheet(speed: f64) -> String {
    format!(
        ":root {{ --gl-duration: {}ms; }}",
        scaled(DURATION_MS, speed)
    )
}

pub struct Sheets {
    pub theme: Option<PathBuf>,
    pub theme_dark: Option<PathBuf>,
    pub dropin: Option<PathBuf>,
    pub dropin_dark: Option<PathBuf>,
}

struct Dark {
    theme: CssProvider,
    theme_path: RefCell<Option<PathBuf>>,
    dropin: CssProvider,
    dropin_path: RefCell<Option<PathBuf>>,
    applied: Cell<bool>,
}

impl Dark {
    fn sync(&self, dark: bool, force: bool) {
        if !force && self.applied.get() == dark {
            return;
        }
        self.applied.set(dark);
        let theme = self.theme_path.borrow();
        let dropin = self.dropin_path.borrow();
        let (theme, dropin) = if dark {
            (theme.as_deref(), dropin.as_deref())
        } else {
            (None, None)
        };
        load("theme dark", &self.theme, theme);
        load("drop-in dark", &self.dropin, dropin);
    }
}

pub struct Styles {
    builtin: CssProvider,
    speed: CssProvider,
    theme: CssProvider,
    dropin: CssProvider,
    dark: Rc<Dark>,
    variant: RefCell<String>,
    style_manager: adw::StyleManager,
    dark_handler: Option<glib::SignalHandlerId>,
}

impl Styles {
    pub fn install(scheme: adw::ColorScheme) -> Self {
        let builtin = CssProvider::new();
        let speed = CssProvider::new();
        let theme = CssProvider::new();
        let dropin = CssProvider::new();
        let dark = Rc::new(Dark {
            theme: CssProvider::new(),
            theme_path: RefCell::new(None),
            dropin: CssProvider::new(),
            dropin_path: RefCell::new(None),
            applied: Cell::new(false),
        });
        for provider in [&builtin, &speed, &theme, &dropin, &dark.theme, &dark.dropin] {
            report_parsing_errors(provider);
        }
        builtin.load_from_string(BUILTIN);
        speed.load_from_string(&speed_sheet(SPEED.get()));

        match gdk::Display::default() {
            Some(display) => {
                gtk4::IconTheme::for_display(&display)
                    .add_resource_path("/me/aresa/GlimpseShell/icons");
                gtk4::style_context_add_provider_for_display(&display, &builtin, BUILTIN_PRIORITY);
                gtk4::style_context_add_provider_for_display(&display, &theme, THEME_PRIORITY);
                gtk4::style_context_add_provider_for_display(
                    &display,
                    &dark.theme,
                    THEME_DARK_PRIORITY,
                );
                gtk4::style_context_add_provider_for_display(&display, &dropin, DROPIN_PRIORITY);
                gtk4::style_context_add_provider_for_display(
                    &display,
                    &dark.dropin,
                    DROPIN_DARK_PRIORITY,
                );
                gtk4::style_context_add_provider_for_display(&display, &speed, SPEED_PRIORITY);
            }
            None => tracing::error!("no display; stylesheets will not be applied"),
        }

        let style_manager = adw::StyleManager::default();
        style_manager.set_color_scheme(scheme);
        let dark_handler = style_manager.connect_dark_notify({
            let builtin = builtin.clone();
            let theme = theme.clone();
            let dropin = dropin.clone();
            let dark = dark.clone();
            move |manager| {
                let is_dark = manager.is_dark();
                set_provider_scheme(
                    provider_scheme(is_dark),
                    [&builtin, &theme, &dark.theme, &dropin, &dark.dropin],
                );
                dark.sync(is_dark, false);
            }
        });
        let styles = Self {
            builtin,
            speed,
            theme,
            dropin,
            dark,
            variant: RefCell::new(String::new()),
            style_manager,
            dark_handler: Some(dark_handler),
        };
        styles.sync_provider_scheme();
        tracing::info!(
            requested = requested_scheme(scheme),
            effective = effective_scheme(styles.style_manager.is_dark()),
            "color scheme"
        );
        styles
    }

    pub fn load(&self, sheets: &Sheets) {
        load("theme", &self.theme, sheets.theme.as_deref());
        load("drop-in", &self.dropin, sheets.dropin.as_deref());
        self.dark.theme_path.replace(sheets.theme_dark.clone());
        self.dark.dropin_path.replace(sheets.dropin_dark.clone());
        self.dark.sync(self.style_manager.is_dark(), true);
    }

    pub fn set_variant(&self, variant: &str) {
        let wanted = usable_variant(variant);
        let previous = self.variant.replace(wanted.clone());
        for window in gtk4::Window::list_toplevels() {
            if !previous.is_empty() && previous != wanted {
                window.remove_css_class(&previous);
            }
            if !wanted.is_empty() {
                window.add_css_class(&wanted);
            }
        }
    }

    pub fn set_animation_speed(&self, speed: f64) {
        if SPEED.replace(speed) == speed {
            return;
        }
        self.speed.load_from_string(&speed_sheet(speed));
    }

    pub fn set_color_scheme(&self, scheme: adw::ColorScheme) {
        self.style_manager.set_color_scheme(scheme);
        self.sync_provider_scheme();
    }

    fn sync_provider_scheme(&self) {
        set_provider_scheme(
            provider_scheme(self.style_manager.is_dark()),
            [
                &self.builtin,
                &self.theme,
                &self.dark.theme,
                &self.dropin,
                &self.dark.dropin,
            ],
        );
    }
}

impl Drop for Styles {
    fn drop(&mut self) {
        if let Some(handler) = self.dark_handler.take() {
            self.style_manager.disconnect(handler);
        }
    }
}

fn usable_variant(variant: &str) -> String {
    if variant.is_empty() {
        return String::new();
    }
    let usable = variant
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !variant.starts_with(|c: char| c.is_ascii_digit());
    if usable {
        variant.to_owned()
    } else {
        tracing::warn!(
            variant,
            "theme-variant is not a usable CSS class; ignoring it"
        );
        String::new()
    }
}

fn provider_scheme(dark: bool) -> gtk4::InterfaceColorScheme {
    if dark {
        gtk4::InterfaceColorScheme::Dark
    } else {
        gtk4::InterfaceColorScheme::Light
    }
}

fn requested_scheme(scheme: adw::ColorScheme) -> &'static str {
    match scheme {
        adw::ColorScheme::Default => "auto",
        adw::ColorScheme::ForceLight => "light",
        adw::ColorScheme::PreferLight => "prefer-light",
        adw::ColorScheme::PreferDark => "prefer-dark",
        adw::ColorScheme::ForceDark => "dark",
        _ => "unknown",
    }
}

fn effective_scheme(dark: bool) -> &'static str {
    if dark { "dark" } else { "light" }
}

fn set_provider_scheme(scheme: gtk4::InterfaceColorScheme, providers: [&CssProvider; 5]) {
    for provider in providers {
        provider.set_prefers_color_scheme(scheme);
    }
}

fn load(role: &str, provider: &CssProvider, path: Option<&Path>) {
    match path {
        Some(path) => {
            tracing::debug!(role, path = %path.display(), "loading stylesheet");
            provider.load_from_path(path);
        }
        None => {
            tracing::debug!(role, "no stylesheet; clearing its provider");
            provider.load_from_string("");
        }
    }
}

fn report_parsing_errors(provider: &CssProvider) {
    provider.connect_parsing_error(|_, section, error| {
        tracing::error!(at = %section.to_str(), %error, "stylesheet");
    });
}

#[cfg(test)]
mod tests;
