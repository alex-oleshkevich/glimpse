use std::path::Path;

use adw::prelude::*;
use gtk4::{CssProvider, InterfaceColorScheme, gdk, glib};

pub const BUILTIN: &str = include_str!("../styles/glimpse.css");

const BUILTIN_PRIORITY: u32 = gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION;
const THEME_PRIORITY: u32 = gtk4::STYLE_PROVIDER_PRIORITY_USER;
const DROPIN_PRIORITY: u32 = THEME_PRIORITY + 1;

pub struct Styles {
    theme: CssProvider,
    dropin: CssProvider,
    style_manager: adw::StyleManager,
    color_scheme_handler: Option<glib::SignalHandlerId>,
}

impl Styles {
    pub fn install() -> Self {
        let builtin = CssProvider::new();
        let theme = CssProvider::new();
        let dropin = CssProvider::new();
        report_parsing_errors(&builtin);
        report_parsing_errors(&theme);
        report_parsing_errors(&dropin);
        builtin.load_from_string(BUILTIN);

        let style_manager = adw::StyleManager::default();
        let providers = [builtin.clone(), theme.clone(), dropin.clone()];
        set_color_scheme(&providers, style_manager.is_dark());
        let color_scheme_handler = style_manager.connect_dark_notify(move |manager| {
            set_color_scheme(&providers, manager.is_dark());
        });

        match gdk::Display::default() {
            Some(display) => {
                gtk4::style_context_add_provider_for_display(&display, &builtin, BUILTIN_PRIORITY);
                gtk4::style_context_add_provider_for_display(&display, &theme, THEME_PRIORITY);
                gtk4::style_context_add_provider_for_display(&display, &dropin, DROPIN_PRIORITY);
            }
            None => tracing::error!("no display; stylesheets will not be applied"),
        }

        Self {
            theme,
            dropin,
            style_manager,
            color_scheme_handler: Some(color_scheme_handler),
        }
    }

    pub fn load(&self, theme: Option<&Path>, dropin: Option<&Path>) {
        load("theme", &self.theme, theme);
        load("drop-in", &self.dropin, dropin);
    }
}

impl Drop for Styles {
    fn drop(&mut self) {
        if let Some(handler) = self.color_scheme_handler.take() {
            self.style_manager.disconnect(handler);
        }
    }
}

fn set_color_scheme(providers: &[CssProvider], dark: bool) {
    let scheme = interface_color_scheme(dark);
    for provider in providers {
        provider.set_prefers_color_scheme(scheme);
    }
}

fn interface_color_scheme(dark: bool) -> InterfaceColorScheme {
    if dark {
        InterfaceColorScheme::Dark
    } else {
        InterfaceColorScheme::Light
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
    use super::{BUILTIN, InterfaceColorScheme, interface_color_scheme};

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
    fn notification_surface_ramp_is_tokenized_for_both_schemes() {
        assert!(BUILTIN.contains("--gl-notification: #f2f2f2;"));
        assert!(BUILTIN.contains(
            "--gl-notification-back: mix(var(--gl-surface), var(--gl-surface-fg), 0.14);"
        ));
        assert!(BUILTIN.contains(
            "--gl-notification-back-far: mix(var(--gl-surface), var(--gl-surface-fg), 0.24);"
        ));
        assert!(BUILTIN.contains("--gl-notification: #54545a;"));
        assert!(BUILTIN.contains("--gl-notification-back: #45454a;"));
        assert!(BUILTIN.contains("--gl-notification-back-far: #3d3d42;"));
        assert!(BUILTIN.contains("background-color: var(--gl-notification-back);"));
        assert!(BUILTIN.contains("background-color: var(--gl-notification-back-far);"));
        for color in ["#f2f2f2", "#54545a", "#45454a", "#3d3d42"] {
            assert_eq!(
                BUILTIN.matches(color).count(),
                1,
                "the notification palette declares {color} more than once"
            );
        }
    }

    #[test]
    fn attention_colors_the_dot_without_coloring_the_icon() {
        let reset = BUILTIN
            .find(".indicator--attention .indicator__icon")
            .expect("attention keeps indicator content neutral");
        let error = BUILTIN
            .find(".indicator--error .indicator__icon")
            .expect("error colors indicator content when there is no attention dot");
        assert!(reset > error, "the attention reset overrides severity");
        assert!(BUILTIN.contains(
            ".indicator--error .indicator__attention-dot {\n    background-color: var(--gl-danger-text);"
        ));
    }

    #[test]
    fn notification_actions_leave_room_below_the_buttons() {
        assert!(BUILTIN.contains("margin: 0.27rem 1.08rem 0.8rem 1.08rem;"));
    }

    #[test]
    fn notification_group_header_is_separated_from_its_card() {
        assert!(BUILTIN.contains(
            ".notifications-popover__group .section__header {\n    margin-bottom: 0.4rem;\n}"
        ));
    }

    #[test]
    fn notification_cards_use_raised_elevation_inside_the_popover() {
        assert!(BUILTIN.contains(
            ".notifications-popover__group .notification {\n    box-shadow: var(--gl-elevation-raised);\n}"
        ));
    }

    #[test]
    fn provider_scheme_follows_the_resolved_adwaita_scheme() {
        assert_eq!(interface_color_scheme(false), InterfaceColorScheme::Light);
        assert_eq!(interface_color_scheme(true), InterfaceColorScheme::Dark);
    }

    #[test]
    fn the_declared_vocabulary_is_the_documented_size() {
        let (block, _) = split(BUILTIN);
        assert_eq!(declared(block).len(), 34);
    }
}
