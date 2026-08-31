use std::path::Path;

use gtk4::{CssProvider, gdk};

pub const BUILTIN: &str = include_str!("../styles/glimpse.css");

const BUILTIN_PRIORITY: u32 = gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION;
const THEME_PRIORITY: u32 = gtk4::STYLE_PROVIDER_PRIORITY_USER;
const DROPIN_PRIORITY: u32 = THEME_PRIORITY + 1;

pub struct Styles {
    theme: CssProvider,
    dropin: CssProvider,
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

        match gdk::Display::default() {
            Some(display) => {
                gtk4::style_context_add_provider_for_display(&display, &builtin, BUILTIN_PRIORITY);
                gtk4::style_context_add_provider_for_display(&display, &theme, THEME_PRIORITY);
                gtk4::style_context_add_provider_for_display(&display, &dropin, DROPIN_PRIORITY);
            }
            None => tracing::error!("no display; stylesheets will not be applied"),
        }

        Self { theme, dropin }
    }

    pub fn load(&self, theme: Option<&Path>, dropin: Option<&Path>) {
        load("theme", &self.theme, theme);
        load("drop-in", &self.dropin, dropin);
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
    use super::BUILTIN;

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
        let declared = declared(block);
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
    fn no_rule_names_a_literal_color() {
        let (_, rules) = split(BUILTIN);
        for line in rules.lines() {
            let line = line.trim();
            assert!(
                !line.contains('#') && !line.contains("rgb(") && !line.contains("rgba("),
                "a rule names a literal color, which cannot follow a theme: {line}"
            );
        }
    }

    #[test]
    fn the_declared_vocabulary_is_the_documented_size() {
        let (block, _) = split(BUILTIN);
        assert_eq!(declared(block).len(), 26);
    }
}
