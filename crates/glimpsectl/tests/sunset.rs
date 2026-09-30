use glimpse_dbus::night_light::NightLightSnapshot;
use glimpsectl::commands::sunset::{mode, serving, temperature};

fn snapshot() -> NightLightSnapshot {
    NightLightSnapshot {
        schedule: "automatic".to_owned(),
        overridden: false,
        temperature: 6500,
        target: 4200,
        active: false,
        serving: true,
        reason: String::new(),
        configured: "automatic".to_owned(),
        manual: false,
    }
}

#[test]
fn an_overridden_mode_says_that_the_document_is_not_the_one_talking() {
    let mut overridden = snapshot();
    overridden.schedule = "off".to_owned();
    overridden.overridden = true;

    assert!(mode(&overridden).contains("off"));
    assert!(mode(&overridden).contains("override"));
    assert!(!mode(&snapshot()).contains("override"));
}

#[test]
fn a_manual_temperature_says_so_and_a_scheduled_one_does_not() {
    let mut manual = snapshot();
    manual.manual = true;

    assert!(temperature(&manual).contains("manual"));
    assert!(!temperature(&snapshot()).contains("manual"));
}

#[test]
fn a_provider_that_is_not_serving_shows_why() {
    let mut degraded = snapshot();
    degraded.serving = false;
    degraded.reason = "another gamma client holds the outputs".to_owned();

    assert!(serving(&degraded).contains("another gamma client"));
    assert!(!serving(&snapshot()).contains('-'));
}
