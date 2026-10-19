use super::*;

fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, text).expect("writes");
    crate::load(Some(&path))
}

#[test]
fn an_absent_table_follows_the_environment() {
    let regional = load("").expect("an absent table is fine").regional;

    assert_eq!(regional.hour_format, HourFormat::Locale);
    assert_eq!(regional.units, Units::Locale);
    assert_eq!(regional.language(), None);
}

#[test]
fn every_key_reads_kebab_case() {
    let parsed =
        load("[regional]\nlanguage = \"ru\"\nhour-format = \"12h\"\nunits = \"imperial\"\n")
            .expect("kebab-case keys")
            .regional;

    assert_eq!(parsed.language(), Some("ru"));
    assert_eq!(parsed.hour_format, HourFormat::Twelve);
    assert_eq!(parsed.units, Units::Imperial);
}

#[test]
fn an_explicit_setting_answers_without_asking_the_environment() {
    let twelve = Regional {
        hour_format: HourFormat::Twelve,
        units: Units::Imperial,
        ..Regional::default()
    };
    assert!(twelve.twelve_hour());
    assert!(!twelve.is_metric());
    assert_eq!(environment::clock(twelve.twelve_hour()), "%-I:%M %p");

    let twenty_four = Regional {
        hour_format: HourFormat::TwentyFour,
        units: Units::Metric,
        ..Regional::default()
    };
    assert!(!twenty_four.twelve_hour());
    assert!(twenty_four.is_metric());
    assert_eq!(environment::clock(twenty_four.twelve_hour()), "%H:%M");
}

#[test]
fn a_blank_language_is_the_same_as_an_absent_one() {
    assert_eq!(
        Regional {
            language: "   ".to_owned(),
            ..Regional::default()
        }
        .language(),
        None
    );
    assert_eq!(
        Regional {
            language: " ru ".to_owned(),
            ..Regional::default()
        }
        .language(),
        Some("ru")
    );
}

#[test]
fn it_is_not_an_applet_setting() {
    load("[applets.clock]\nregional = {}\n").expect_err("`regional` is not an applet setting");

    let applet = crate::Applet::from_name("clock").expect("the clock is an applet");
    let rendered = toml::to_string(&applet).expect("an applet serializes");
    assert!(
        !rendered.contains("regional"),
        "the copy must not round-trip, got {rendered}"
    );
}

#[test]
fn an_unknown_value_is_refused_rather_than_falling_back() {
    load("[regional]\nunits = \"kelvin\"\n").expect_err("`kelvin` is not a unit system");
    load("[regional]\nhour-format = \"36h\"\n").expect_err("`36h` is not an hour format");
    load("[regional]\nlanguages = \"ru\"\n").expect_err("`languages` is not a key");
}
