use super::*;

#[test]
fn failure_is_the_verdict() {
    let problems = Problems {
        failure: Some("failed".to_owned()),
        rejected: Some("rejected".to_owned()),
        ..Problems::default()
    };
    assert_eq!(problems.verdict(), Err("failed".to_owned()));
}

#[test]
fn rejection_is_the_verdict_without_failure() {
    let problems = Problems {
        rejected: Some("rejected".to_owned()),
        ..Problems::default()
    };
    assert_eq!(problems.verdict(), Err("rejected".to_owned()));
}

#[test]
fn unknown_applets_do_not_fail_the_verdict() {
    let problems = Problems {
        unknown_applets: vec!["unknown".to_owned()],
        ..Problems::default()
    };
    assert_eq!(problems.verdict(), Ok(()));
}

#[test]
fn empty_problems_have_an_ok_verdict() {
    assert_eq!(Problems::default().verdict(), Ok(()));
}

#[test]
fn a_clean_document_has_nothing_to_say() {
    assert_eq!(Problems::default().body(), None);
}

#[test]
fn a_failed_load_says_what_is_off_and_every_problem_is_listed() {
    let problems = Problems {
        failure: Some("config.toml: unknown field `panle`".to_owned()),
        rejected: None,
        unknown_applets: vec!["unknown applet `command-test`".to_owned()],
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
