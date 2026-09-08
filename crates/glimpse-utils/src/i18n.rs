use std::env;
use std::path::PathBuf;

use gettextrs::{LocaleCategory, bind_textdomain_codeset, bindtextdomain, setlocale, textdomain};

const DOMAIN: &str = "glimpse";
const INSTALLED: &str = env!("GLIMPSE_LOCALE_DIR_DEFAULT");
const OVERRIDE: &str = "GLIMPSE_LOCALE_DIR";
const LANGUAGE: &str = "LANGUAGE";

fn locale_dir() -> PathBuf {
    env::var_os(OVERRIDE).map_or_else(|| PathBuf::from(INSTALLED), PathBuf::from)
}

/// A GTK template resolves its text as each widget is built, so re-binding the domain now would
/// leave what is already on screen in the old language and everything opened afterwards in the
/// new one. Saying so is the whole handling.
pub fn report_language_change(previous: Option<&str>, current: Option<&str>) {
    if previous == current {
        return;
    }
    tracing::info!(
        language = current.unwrap_or("<environment>"),
        "language changed; it applies when this binary next starts"
    );
}

pub fn init_locale() {
    if unsafe { setlocale(LocaleCategory::LcAll, "") }.is_none() {
        tracing::warn!(
            "the locale named by the environment is not available; text stays in English"
        );
    }
}

pub fn init_translations(language: Option<&str>) {
    if let Some(language) = language
        && env::var_os(LANGUAGE).is_none()
    {
        unsafe { env::set_var(LANGUAGE, language) };
    }

    init_locale();

    let dir = locale_dir();
    let bound = bindtextdomain(DOMAIN, &dir)
        .and_then(|_| bind_textdomain_codeset(DOMAIN, "UTF-8"))
        .and_then(|_| textdomain(DOMAIN));

    if let Err(error) = bound {
        tracing::warn!(%error, directory = %dir.display(), "translations are unavailable; text stays in English");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One test, because `set_var` is process-global and cargo runs tests on parallel threads: a
    /// second test touching `GLIMPSE_LOCALE_DIR` would race this one for it.
    #[test]
    fn the_catalog_directory_is_the_install_prefix_unless_the_environment_names_another() {
        // SAFETY: this is the only test in the crate that touches this variable, and it is
        // restored before returning.
        unsafe { env::remove_var(OVERRIDE) };
        assert_eq!(locale_dir(), PathBuf::from(INSTALLED));

        unsafe { env::set_var(OVERRIDE, "/nonexistent/glimpse/locale") };
        assert_eq!(locale_dir(), PathBuf::from("/nonexistent/glimpse/locale"));

        init_translations(None);

        unsafe { env::remove_var(OVERRIDE) };
    }
}
