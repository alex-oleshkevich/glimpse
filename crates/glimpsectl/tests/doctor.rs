use std::path::PathBuf;

use glimpsectl::commands::doctor::{Configuration, Provider};

#[test]
fn a_provider_that_answers_but_is_not_serving_is_not_reported_healthy() {
    let degraded = Provider::answered(
        "x",
        false,
        "another gamma client holds the outputs".to_owned(),
    );
    assert!(degraded.running, "it answered, so it is running");
    let [_, state, detail] = degraded.row();
    assert!(state.contains("degraded"));
    assert!(detail.contains("another gamma client"));
}

#[test]
fn a_provider_that_owns_its_name_but_never_answers_is_degraded_not_missing() {
    let hung = Provider::answered(
        "me.aresa.Glimpse.NightLight",
        false,
        "did not answer within 10s".to_owned(),
    );
    let missing = Provider::absent(
        "me.aresa.Glimpse.Weather",
        "nobody owns the name".to_owned(),
    );

    assert!(hung.running, "it owns the name, it just will not talk");
    let [_, hung_state, detail] = hung.row();
    let [_, missing_state, _] = missing.row();

    assert!(hung_state.contains("degraded"), "{hung_state}");
    assert!(missing_state.contains("not running"), "{missing_state}");
    assert_ne!(hung_state, missing_state);
    assert!(detail.contains("did not answer"));
}

#[test]
fn a_serving_provider_has_no_detail_to_add() {
    let [_, state, detail] = Provider::answered("x", true, String::new()).row();
    assert!(state.contains("serving"));
    assert!(detail.is_empty());
}

#[test]
fn an_absent_provider_is_not_running_and_says_why() {
    let absent = Provider::absent("x", "nobody owns the name".to_owned());
    assert!(!absent.running);
    let [_, state, detail] = absent.row();
    assert!(state.contains("not running"));
    assert!(detail.contains("nobody owns the name"));
}

#[test]
fn a_configuration_that_does_not_load_says_binaries_will_not_start() {
    let broken = Configuration {
        files: vec![PathBuf::from("/etc/glimpse/config.toml")],
        loads: false,
        error: Some("expected a table".to_owned()),
    };
    let rendered = broken.section().render();
    assert!(rendered.contains("expected a table"));
    assert!(rendered.contains("no binary starts"));
    assert!(!rendered.contains("falls back to defaults\n"));
}
