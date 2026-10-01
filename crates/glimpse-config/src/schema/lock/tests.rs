use super::{Privacy, SessionAction};

fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, text).expect("writes");
    crate::load(Some(&path))
}

#[test]
fn the_defaults_match_the_stage_design() {
    let config = load("").expect("defaults load").lock;
    assert_eq!(config.pam_service, "glimpse-lock");
    assert_eq!(config.prompt_output, "focused");
    assert_eq!(config.background.color, "#000000");
    assert_eq!(config.background.fit, crate::Fit::Cover);
    assert_eq!(config.background.blur_radius, 0);
    assert_eq!(config.background.dim, 0.35);
    assert_eq!(config.background.dim_dark, 0.5);
    assert_eq!(config.clock.date_format, "%A, {day} %B");
    assert!(config.status.enabled);
    assert!(config.media.enabled);
    assert!(config.notifications.enabled);
    assert_eq!(config.notifications.privacy, Privacy::Apps);
    assert!(config.session.enabled);
    assert_eq!(
        config.session.actions,
        vec![
            SessionAction::Suspend,
            SessionAction::Reboot,
            SessionAction::PowerOff,
        ]
    );
}

#[test]
fn blur_radius_outside_its_range_is_refused() {
    let error = load("[lock.background]\nblur-radius = 129\n")
        .expect_err("out of range")
        .to_string();
    assert!(
        error.contains("must be between 0 and 128 pixels"),
        "{error}"
    );
}

#[test]
fn dim_above_one_is_refused() {
    let error = load("[lock.background]\ndim = 1.5\n")
        .expect_err("out of range")
        .to_string();
    assert!(error.contains("must be between 0.0 and 1.0"), "{error}");
}

#[test]
fn dim_below_zero_is_refused() {
    let error = load("[lock.background]\ndim = -0.1\n")
        .expect_err("out of range")
        .to_string();
    assert!(error.contains("must be between 0.0 and 1.0"), "{error}");
}

#[test]
fn dim_nan_is_refused() {
    let error = load("[lock.background]\ndim = nan\n")
        .expect_err("NaN is not a fraction")
        .to_string();
    assert!(error.contains("must be between 0.0 and 1.0"), "{error}");
}

#[test]
fn dim_dark_outside_its_range_is_refused() {
    let error = load("[lock.background]\ndim-dark = 1.5\n")
        .expect_err("out of range")
        .to_string();
    assert!(error.contains("must be between 0.0 and 1.0"), "{error}");
}

#[test]
fn an_unknown_session_action_is_refused() {
    let error = load("[lock.session]\nactions = [\"hibernate\"]\n")
        .expect_err("hibernate is not an action")
        .to_string();
    assert!(error.contains("hibernate"), "{error}");
}

#[test]
fn an_unknown_privacy_level_is_refused() {
    let error = load("[lock.notifications]\nprivacy = \"summaries\"\n")
        .expect_err("summaries is not a privacy level")
        .to_string();
    assert!(error.contains("summaries"), "{error}");
}

#[test]
fn the_old_flat_background_key_is_refused() {
    let error = load("[lock]\nbackground = \"x\"\n")
        .expect_err("background is a table now, not a string")
        .to_string();
    assert!(
        error.contains("invalid type: string \"x\", expected struct Background"),
        "{error}"
    );
}

#[test]
fn a_flat_dim_directly_under_lock_is_refused() {
    let error = load("[lock]\ndim = 0.5\n")
        .expect_err("dim moved under [lock.background]")
        .to_string();
    assert!(error.contains("unknown field `dim`"), "{error}");
}

#[test]
fn the_old_controls_table_is_refused() {
    let error = load("[lock.controls]\nbuttons = []\n")
        .expect_err("controls is gone")
        .to_string();
    assert!(error.contains("controls"), "{error}");
}
