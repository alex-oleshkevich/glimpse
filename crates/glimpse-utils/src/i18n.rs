use std::env;
use std::path::PathBuf;

use gettextrs::{LocaleCategory, bind_textdomain_codeset, bindtextdomain, setlocale, textdomain};

const DOMAIN: &str = "glimpse";
const INSTALLED: &str = "/usr/share/locale";
const OVERRIDE: &str = "GLIMPSE_LOCALE_DIR";

pub fn locale_dir() -> PathBuf {
    env::var_os(OVERRIDE).map_or_else(|| PathBuf::from(INSTALLED), PathBuf::from)
}

pub fn init_translations() {
    if unsafe { setlocale(LocaleCategory::LcAll, "") }.is_none() {
        tracing::warn!("the locale could not be set; text stays in English");
    }

    let dir = locale_dir();
    if let Err(error) = bindtextdomain(DOMAIN, &dir) {
        tracing::warn!(%error, directory = %dir.display(), "no message catalogs; text stays in English");
        return;
    }
    if let Err(error) = bind_textdomain_codeset(DOMAIN, "UTF-8") {
        tracing::warn!(%error, "the message catalog encoding could not be set");
    }
    if let Err(error) = textdomain(DOMAIN) {
        tracing::warn!(%error, "the text domain could not be selected; text stays in English");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_installed_directory_is_used_unless_the_environment_names_another() {
        // SAFETY: the two `set_var`/`remove_var` calls bracket every read in this test, and no
        // other test in this crate reads `GLIMPSE_LOCALE_DIR`.
        unsafe { env::remove_var(OVERRIDE) };
        assert_eq!(locale_dir(), PathBuf::from(INSTALLED));

        unsafe { env::set_var(OVERRIDE, "/tmp/nowhere/locale") };
        assert_eq!(locale_dir(), PathBuf::from("/tmp/nowhere/locale"));

        unsafe { env::remove_var(OVERRIDE) };
    }

    /// A catalog directory that is not there is an English shell, not a dead one — the same call
    /// a service makes for a missing bus. The gtk-rs book's `.expect()` is fine for one
    /// application and wrong for a suite of six.
    #[test]
    fn a_missing_catalog_directory_is_survivable() {
        // SAFETY: as above.
        unsafe { env::set_var(OVERRIDE, "/nonexistent/glimpse/locale") };
        init_translations();
        unsafe { env::remove_var(OVERRIDE) };
    }
}
