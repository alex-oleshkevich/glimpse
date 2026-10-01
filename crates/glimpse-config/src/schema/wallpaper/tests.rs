fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, text).expect("writes");
    crate::load(Some(&path))
}

#[test]
fn transition_ms_outside_its_range_is_refused() {
    let error = load("[wallpaper]\ntransition-ms = 99999\n")
        .expect_err("out of range")
        .to_string();
    assert!(
        error.contains("must be between 0 and 5000 milliseconds"),
        "{error}"
    );
}

#[test]
fn blur_radius_outside_its_range_is_refused() {
    let error = load("[wallpaper.backdrop]\nblur-radius = 9999\n")
        .expect_err("out of range")
        .to_string();
    assert!(
        error.contains("must be between 0 and 128 pixels"),
        "{error}"
    );
}

#[test]
fn downscale_factor_defaults_to_four_and_refuses_zero() {
    assert_eq!(
        load("")
            .expect("defaults")
            .wallpaper
            .backdrop
            .downscale_factor,
        4
    );
    let error = load("[wallpaper.backdrop]\ndownscale-factor = 0\n")
        .expect_err("out of range")
        .to_string();
    assert!(error.contains("must be between 1 and 16"), "{error}");
}

#[test]
fn fit_accepts_center() {
    let config = load("[wallpaper]\nfit = \"center\"\n").expect("center is a valid fit");
    assert_eq!(config.wallpaper.fit, crate::Fit::Center);
}

#[test]
fn an_absent_outputs_table_is_an_empty_list() {
    let config = load("").expect("an absent table is fine");
    assert!(config.wallpaper.outputs.is_empty());
}

#[test]
fn an_output_override_reads_its_monitor_and_leaves_the_rest_unset() {
    let config = load("[[wallpaper.outputs]]\nmonitor = \"DP-2\"\n")
        .expect("a bare monitor key is enough")
        .wallpaper;
    assert_eq!(config.outputs.len(), 1);
    let output = &config.outputs[0];
    assert_eq!(output.monitor, "DP-2");
    assert_eq!(output.color, None);
    assert_eq!(output.image, None);
    assert_eq!(output.fit, None);
    assert_eq!(output.transition, None);
    assert_eq!(output.transition_ms, None);
    assert_eq!(output.backdrop, None);
}

#[test]
fn an_output_override_without_a_monitor_is_refused() {
    assert!(load("[[wallpaper.outputs]]\nfit = \"contain\"\n").is_err());
}

#[test]
fn an_output_override_with_an_unknown_field_is_refused() {
    let error = load("[[wallpaper.outputs]]\nmonitor = \"DP-2\"\nbogus = 1\n")
        .expect_err("bogus is not a field of this table")
        .to_string();
    assert!(error.contains("bogus"), "{error}");
}

#[test]
fn an_output_override_monitor_must_be_trimmed_and_non_empty() {
    for text in [
        "[[wallpaper.outputs]]\nmonitor = \"\"\n",
        "[[wallpaper.outputs]]\nmonitor = \" DP-2\"\n",
    ] {
        assert!(load(text).is_err(), "must reject {text:?}");
    }
}

#[test]
fn an_output_override_transition_ms_out_of_range_is_refused() {
    let error = load("[[wallpaper.outputs]]\nmonitor = \"DP-2\"\ntransition-ms = 99999\n")
        .expect_err("out of range")
        .to_string();
    assert!(
        error.contains("must be between 0 and 5000 milliseconds"),
        "{error}"
    );
}

#[test]
fn an_output_override_reads_every_field_including_its_nested_backdrop() {
    let config = load(concat!(
        "[[wallpaper.outputs]]\n",
        "monitor = \"DP-2\"\n",
        "color = \"#112233\"\n",
        "image = \"city.jpg\"\n",
        "image-dark = \"city-dark.jpg\"\n",
        "fit = \"contain\"\n",
        "transition = \"none\"\n",
        "transition-ms = 250\n",
        "\n",
        "[wallpaper.outputs.backdrop]\n",
        "enabled = true\n",
        "blur-radius = 40\n",
    ))
    .expect("every field loads")
    .wallpaper;

    let output = &config.outputs[0];
    assert_eq!(output.monitor, "DP-2");
    assert_eq!(output.color.as_deref(), Some("#112233"));
    assert_eq!(output.image, Some(std::path::PathBuf::from("city.jpg")));
    assert_eq!(
        output.image_dark,
        Some(std::path::PathBuf::from("city-dark.jpg"))
    );
    assert_eq!(output.fit, Some(crate::Fit::Contain));
    assert_eq!(output.transition, Some(crate::Transition::None));
    assert_eq!(output.transition_ms, Some(250));
    let backdrop = output.backdrop.as_ref().expect("backdrop override loads");
    assert_eq!(backdrop.enabled, Some(true));
    assert_eq!(backdrop.blur_radius, Some(40));
}

#[test]
fn two_output_overrides_for_different_connectors_both_load() {
    let config = load(concat!(
        "[[wallpaper.outputs]]\n",
        "monitor = \"DP-2\"\n",
        "\n",
        "[[wallpaper.outputs]]\n",
        "monitor = \"eDP-1\"\n",
    ))
    .expect("two distinct connectors load")
    .wallpaper;

    assert_eq!(config.outputs.len(), 2);
    assert_eq!(config.outputs[0].monitor, "DP-2");
    assert_eq!(config.outputs[1].monitor, "eDP-1");
}
