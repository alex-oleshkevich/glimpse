use super::*;

#[test]
fn the_catalog_directory_is_the_install_prefix_unless_the_environment_names_another() {
    unsafe { env::remove_var(OVERRIDE) };
    assert_eq!(locale_dir(), PathBuf::from(INSTALLED));

    unsafe { env::set_var(OVERRIDE, "/nonexistent/glimpse/locale") };
    assert_eq!(locale_dir(), PathBuf::from("/nonexistent/glimpse/locale"));

    init_translations(None);

    unsafe { env::remove_var(OVERRIDE) };
}
