use super::*;

#[test]
fn unknown_applets_do_not_fail_the_verdict() {
    let problems = Problems {
        unknown_applets: vec!["unknown".to_owned()],
        ..Problems::default()
    };
    assert_eq!(problems.verdict(), Ok(()));
}

#[test]
fn a_clean_document_has_nothing_to_say() {
    assert_eq!(Problems::default().body(), None);
}

#[test]
fn a_failed_load_says_what_is_off_and_every_problem_is_listed() {
    let problems = Problems {
        failure: Some("config.toml: unknown field `panle`".to_owned()),
        unknown_applets: vec!["unknown applet `command-test`".to_owned()],
        ..Problems::default()
    };
    let body = problems.body().expect("there is something to say");
    assert!(body.contains("panle"));
    assert!(body.contains("Idle and night light are off"));
    assert!(body.contains("command-test"));
}

#[test]
fn a_rejected_reload_while_on_defaults_replaces_the_failure() {
    let mut problems = Problems {
        failure: Some("first".to_owned()),
        ..Problems::default()
    };
    problems.reject("second".to_owned());
    assert_eq!(problems.failure.as_deref(), Some("second"));
    assert_eq!(problems.rejected, None);

    problems.recover();
    problems.reject("third".to_owned());
    assert_eq!(problems.rejected.as_deref(), Some("third"));

    problems.recover();
    assert_eq!(problems.body(), None);
}

#[test]
fn a_theme_problem_is_the_verdict_after_the_config_problems() {
    let mut problems = Problems {
        theme: Some("theme".to_owned()),
        ..Problems::default()
    };
    assert_eq!(problems.verdict(), Err("theme".to_owned()));

    problems.rejected = Some("rejected".to_owned());
    assert_eq!(problems.verdict(), Err("rejected".to_owned()));

    problems.failure = Some("failed".to_owned());
    assert_eq!(problems.verdict(), Err("failed".to_owned()));
}

#[test]
fn a_config_recovery_leaves_the_theme_problem() {
    let mut problems = Problems {
        rejected: Some("rejected".to_owned()),
        theme: Some("styles.css: permission denied".to_owned()),
        ..Problems::default()
    };
    problems.recover();
    assert_eq!(
        problems.verdict(),
        Err("styles.css: permission denied".to_owned())
    );
    let body = problems.body().expect("the theme problem is still there");
    assert!(body.contains("A stylesheet could not be read: styles.css: permission denied"));
}

#[test]
fn a_config_rejection_leaves_the_theme_problem() {
    let mut problems = Problems {
        theme: Some("theme".to_owned()),
        ..Problems::default()
    };
    problems.reject("rejected".to_owned());
    assert_eq!(problems.theme.as_deref(), Some("theme"));
}

#[test]
fn a_broken_panel_document_alone_does_not_claim_idle_and_night_light_are_off() {
    let problems = Problems {
        failure: Some("panel.toml:1:10: unclosed array table".to_owned()),
        panels_only: true,
        ..Problems::default()
    };
    let body = problems.body().expect("there is something to say");
    assert!(body.contains("panel.toml:1:10"));
    assert!(body.contains("The panels use their defaults"));
    assert!(!body.contains("Idle and night light"));
}
