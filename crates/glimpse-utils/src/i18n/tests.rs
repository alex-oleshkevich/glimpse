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
