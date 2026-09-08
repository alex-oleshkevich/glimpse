use std::env;
use std::path::PathBuf;

use gettextrs::{LocaleCategory, bind_textdomain_codeset, bindtextdomain, setlocale, textdomain};

const DOMAIN: &str = "glimpse";
const INSTALLED: &str = env!("GLIMPSE_LOCALE_DIR_DEFAULT");
const OVERRIDE: &str = "GLIMPSE_LOCALE_DIR";

fn locale_dir() -> PathBuf {
    env::var_os(OVERRIDE).map_or_else(|| PathBuf::from(INSTALLED), PathBuf::from)
}

pub fn init_translations() {
    if unsafe { setlocale(LocaleCategory::LcAll, "") }.is_none() {
        tracing::warn!(
            "the locale named by the environment is not available; text stays in English"
        );
    }

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

        init_translations();

        unsafe { env::remove_var(OVERRIDE) };
    }
}
