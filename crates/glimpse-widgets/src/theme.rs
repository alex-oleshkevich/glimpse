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
        let theme = CssProvider::new();
        let dropin = CssProvider::new();
        let dark = Rc::new(Dark {
            theme: CssProvider::new(),
            theme_path: RefCell::new(None),
            dropin: CssProvider::new(),
            dropin_path: RefCell::new(None),
            applied: Cell::new(false),
        });
        for provider in [&builtin, &theme, &dropin, &dark.theme, &dark.dropin] {
            report_parsing_errors(provider);
        }
        builtin.load_from_string(BUILTIN);

        match gdk::Display::default() {
            Some(display) => {
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
mod tests {
    use super::{
        BUILTIN, BUILTIN_PRIORITY, DROPIN_DARK_PRIORITY, DROPIN_PRIORITY, THEME_DARK_PRIORITY,
        THEME_PRIORITY, effective_scheme, provider_scheme, requested_scheme, usable_variant,
    };

    const OPEN: &str = ":root {";
    const PREFIX: &str = "gl-";

    fn split(sheet: &str) -> (&str, &str) {
        let open = sheet.find(OPEN).expect("a :root block");
        let close = open + sheet[open..].find('}').expect("the :root block closes");
        (&sheet[open + OPEN.len()..close], &sheet[close + 1..])
    }

    fn declared(block: &str) -> Vec<&str> {
        block
            .lines()
            .filter_map(|line| line.trim().strip_prefix("--"))
            .filter_map(|line| line.split(':').next())
            .collect()
    }

    fn referenced(text: &str) -> Vec<&str> {
        text.match_indices("var(--")
            .map(|(at, opener)| {
                let rest = &text[at + opener.len()..];
                let end = rest
                    .find(|c: char| !c.is_ascii_alphanumeric() && c != '-')
                    .unwrap_or(rest.len());
                &rest[..end]
            })
            .collect()
    }

    #[test]
    fn every_glimpse_token_the_stylesheet_reads_is_declared() {
        let (block, rules) = split(BUILTIN);
        let declared: Vec<&str> = declared(block).into_iter().chain(declared(rules)).collect();
        let read = referenced(block).into_iter().chain(referenced(rules));

        for name in read.filter(|name| name.starts_with(PREFIX)) {
            assert!(
                declared.contains(&name),
                "--{name} is read but never declared; GTK reports this nowhere"
            );
        }
    }

    #[test]
    fn every_declared_token_carries_the_prefix() {
        let (block, _) = split(BUILTIN);
        for name in declared(block) {
            assert!(
                name.starts_with(PREFIX),
                "--{name} is declared without the --gl- prefix"
            );
        }
    }

    #[test]
    fn no_rule_reads_an_adwaita_token_directly() {
        let (_, rules) = split(BUILTIN);
        for name in referenced(rules) {
            assert!(
                name.starts_with(PREFIX),
                "--{name} is a tier-1 token read from a rule; derive it in :root instead"
            );
        }
    }

    #[test]
    fn no_rule_sets_a_pixel_font_size() {
        let (_, rules) = split(BUILTIN);
        for line in rules.lines().map(str::trim) {
            assert!(
                !line.starts_with("font-size:") || !line.contains("px"),
                "a font size in px cannot follow the user's font: {line}"
            );
        }
    }

    #[test]
    fn only_hairlines_and_borders_are_measured_in_pixels() {
        const BORDERS: [&str; 4] = ["outline:", "outline-offset:", "box-shadow:", "border:"];
        let (_, rules) = split(BUILTIN);

        for line in rules.lines().map(str::trim) {
            if !line.contains("px") {
                continue;
            }
            let border = BORDERS.iter().any(|property| line.starts_with(property));
            let hairline = line.ends_with(": 1px;") || line.ends_with(": 999px;");
            assert!(
                border || hairline,
                "a length in px does not follow the user's text size: {line}"
            );
        }
    }

    #[test]
    fn every_drop_shadow_reads_an_elevation_token() {
        let (_, rules) = split(BUILTIN);
        for line in rules.lines().map(str::trim) {
            let Some(value) = line.strip_prefix("box-shadow:") else {
                continue;
            };
            let value = value.trim();
            assert!(
                value.starts_with("inset ") || value.starts_with("var(--gl-elevation-"),
                "a surface invents its own depth; read an elevation token: {line}"
            );
        }
    }

    #[test]
    fn no_visual_rule_names_a_literal_color() {
        let (_, rules) = split(BUILTIN);
        for line in rules.lines() {
            let line = line.trim();
            if line.starts_with("--gl-") {
                continue;
            }
            assert!(
                !line.contains('#') && !line.contains("rgb(") && !line.contains("rgba("),
                "a rule names a literal color, which cannot follow a theme: {line}"
            );
        }
    }

    #[test]
    fn notification_cards_contrast_with_the_surface_in_both_schemes() {
        assert_eq!(BUILTIN.matches("--gl-notification:").count(), 1);
        assert!(
            BUILTIN
                .contains("--gl-notification: mix(var(--gl-surface), var(--gl-surface-fg), 0.08);")
        );
        assert!(BUILTIN.contains("--gl-notification-fg: var(--gl-surface-fg);"));
        assert!(BUILTIN.contains("color: var(--gl-notification-fg);"));
        assert!(BUILTIN.contains("background-color: var(--gl-notification);"));
    }

    #[test]
    fn providers_follow_the_effective_scheme_without_using_default() {
        assert_eq!(provider_scheme(false), gtk4::InterfaceColorScheme::Light);
        assert_eq!(provider_scheme(true), gtk4::InterfaceColorScheme::Dark);
        assert_eq!(effective_scheme(false), "light");
        assert_eq!(effective_scheme(true), "dark");
        assert_eq!(requested_scheme(adw::ColorScheme::Default), "auto");
        assert_eq!(requested_scheme(adw::ColorScheme::ForceLight), "light");
        assert_eq!(requested_scheme(adw::ColorScheme::ForceDark), "dark");
    }

    #[test]
    fn each_layer_is_refined_by_its_own_dark_sheet_and_the_user_has_the_last_word() {
        const {
            assert!(BUILTIN_PRIORITY < THEME_PRIORITY);
            assert!(THEME_PRIORITY < THEME_DARK_PRIORITY);
            assert!(THEME_DARK_PRIORITY < DROPIN_PRIORITY);
            assert!(DROPIN_PRIORITY < DROPIN_DARK_PRIORITY);
        }
    }

    #[test]
    fn a_variant_that_is_not_a_css_class_is_dropped_rather_than_written() {
        assert_eq!(usable_variant("nord"), "nord");
        assert_eq!(usable_variant("high-contrast_2"), "high-contrast_2");
        assert_eq!(usable_variant(""), "");
        assert_eq!(usable_variant("high contrast"), "");
        assert_eq!(usable_variant("2cool"), "");
        assert_eq!(usable_variant("nord, .panel"), "");
    }

    #[test]
    fn the_declared_vocabulary_is_the_documented_size() {
        let (block, _) = split(BUILTIN);
        assert_eq!(declared(block).len(), 38);
    }

    #[test]
    fn notification_width_and_popup_frame_match_the_measured_surface() {
        assert!(BUILTIN.contains(
            ".notifications-popover .popover-shell,\n.notification {\n    min-width: 40rem;\n}"
        ));
        assert_eq!(BUILTIN.matches("min-width: 40rem;").count(), 1);
        assert!(!BUILTIN.contains("min-width: 42rem;"));
        assert!(BUILTIN.contains("--gl-popup-motion: 0.75rem;"));
        assert!(BUILTIN.contains("--gl-popup-paint-outset: 2.5rem;"));
        assert!(BUILTIN.contains("padding: var(--gl-popup-paint-outset);"));
        assert!(BUILTIN.contains("box-shadow: var(--gl-elevation-floating);"));
        assert!(BUILTIN.contains("transform: translate(0, var(--gl-popup-motion));"));
    }
}
