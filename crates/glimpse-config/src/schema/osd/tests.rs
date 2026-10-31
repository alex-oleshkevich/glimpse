fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, text).expect("writes");
    crate::load_files(std::slice::from_ref(&path))
}

#[test]
fn defaults_enable_a_bottom_center_display_for_1500_milliseconds() {
    let expected = super::Osd {
        enabled: true,
        edge: super::NotificationEdge::BottomCenter,
        duration: 1500,
    };

    assert_eq!(super::Osd::default(), expected);
    assert_eq!(load("").expect("an absent table is fine").osd, expected);
    assert_eq!(
        load("[osd]\n").expect("an empty table is fine").osd,
        expected
    );
}

#[test]
fn duration_accepts_both_bounds() {
    for duration in [500, 10000] {
        let text = format!("[osd]\nduration = {duration}\n");
        assert_eq!(
            load(&text)
                .expect("a duration at either bound loads")
                .osd
                .duration,
            duration
        );
    }
}

#[test]
fn duration_outside_the_bounds_is_refused() {
    for duration in [499, 10001] {
        let text = format!("[osd]\nduration = {duration}\n");
        let rendered = load(&text)
            .expect_err("a duration outside the bounds is refused")
            .to_string();
        assert!(
            rendered.contains("must be between 500 and 10000 milliseconds"),
            "the error must explain the duration bounds, got {rendered}"
        );
    }
}

#[test]
fn a_setting_this_table_does_not_have_is_refused() {
    let rendered = load("[osd]\nfoo = 1\n")
        .expect_err("`foo` is not a key of this table")
        .to_string();

    assert!(
        rendered.contains("foo"),
        "the error must name `foo`, got {rendered}"
    );
}
