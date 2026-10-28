use std::env;
use std::path::PathBuf;

use gettextrs::{LocaleCategory, bind_textdomain_codeset, bindtextdomain, setlocale, textdomain};

const DOMAIN: &str = "glimpse";
const INSTALLED: &str = env!("GLIMPSE_LOCALE_DIR_DEFAULT");
const OVERRIDE: &str = "GLIMPSE_LOCALE_DIR";

fn locale_dir() -> PathBuf {
    env::var_os(OVERRIDE).map_or_else(|| PathBuf::from(INSTALLED), PathBuf::from)
}

pub fn init_locale() {
    if unsafe { setlocale(LocaleCategory::LcAll, "") }.is_none() {
        tracing::warn!(
            "the locale named by the environment is not available; text stays in English"
        );
    }
}

pub fn init_translations() {
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
mod tests;
