use super::*;

fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

fn property<'a>(
    properties: &'a [(&'static str, Value<'static>)],
    name: &str,
) -> &'a Value<'static> {
    &properties
        .iter()
        .find(|(key, _)| *key == name)
        .expect("property")
        .1
}

#[test]
fn transient_names_escape_program_basenames() {
    for (program, escaped) in [
        ("/usr/bin/a-b.c é", "a\\x2db.c\\x20\\xc3\\xa9"),
        (".a_:@\\", "\\x2ea_:\\x40\\x5c"),
    ] {
        let (unit, _) = transient_unit(&[program.to_owned()], "/resolved", "42-7", None, &[]);
        assert_eq!(unit, format!("app-glimpse-{escaped}-42-7.service"));
    }
}

#[test]
fn an_inheriting_unit_keeps_argv_and_carries_the_environment() {
    let command = vec!["a-b".to_owned(), "two words".to_owned(), "$HOME".to_owned()];
    let (_, properties) = transient_unit(
        &command,
        "/usr/bin/a-b",
        "42-7",
        None,
        &pairs(&[("PATH", "/usr/bin"), ("A", "1")]),
    );
    assert_eq!(
        property(&properties, "ExecStart"),
        &Value::from(vec![("/usr/bin/a-b".to_owned(), command, false)])
    );
    assert_eq!(
        property(&properties, "Environment"),
        &Value::from(vec!["PATH=/usr/bin", "A=1"])
    );
    assert_eq!(
        property(&properties, "ExecStart")
            .value_signature()
            .to_string(),
        "a(sasb)"
    );
    assert_eq!(
        property(&properties, "Environment")
            .value_signature()
            .to_string(),
        "as"
    );
}

#[test]
fn a_clean_unit_runs_the_program_under_env_dash_i() {
    let command = vec!["a-b".to_owned(), "two words".to_owned()];
    let (_, properties) = transient_unit(
        &command,
        "/usr/bin/a-b",
        "42-7",
        Some("/usr/bin/env"),
        &pairs(&[("DISPLAY", ":0"), ("A", "1")]),
    );
    let argv: Vec<String> = [
        "env",
        "-i",
        "DISPLAY=:0",
        "A=1",
        "/usr/bin/a-b",
        "two words",
    ]
    .map(str::to_owned)
    .to_vec();
    assert_eq!(
        property(&properties, "ExecStart"),
        &Value::from(vec![("/usr/bin/env".to_owned(), argv, false)])
    );
    assert_eq!(
        property(&properties, "Environment"),
        &Value::from(Vec::<String>::new())
    );
}

#[test]
fn an_inheriting_environment_keeps_allowed_panel_variables_and_yields_to_yours() {
    let vars = BTreeMap::from([("FOO".to_owned(), "mine".to_owned())]);
    let environment = environment(
        pairs(&[
            ("FOO", "panel"),
            ("INVOCATION_ID", "x"),
            ("PATH", "/usr/bin"),
        ]),
        true,
        Some("t"),
        &vars,
    );
    assert_eq!(
        environment,
        pairs(&[
            ("FOO", "mine"),
            ("PATH", "/usr/bin"),
            ("XDG_ACTIVATION_TOKEN", "t"),
        ])
    );
}

#[test]
fn a_clean_environment_holds_only_the_session_variables_the_token_and_yours() {
    let vars = BTreeMap::from([("A".to_owned(), "1".to_owned())]);
    let panel = pairs(&[
        ("HOME", "/h"),
        ("PATH", "/usr/bin"),
        ("XDG_RUNTIME_DIR", "/run/user/1"),
        ("WAYLAND_DISPLAY", "wayland-1"),
    ]);
    assert_eq!(
        environment(panel.clone(), false, None, &vars),
        pairs(&[
            ("XDG_RUNTIME_DIR", "/run/user/1"),
            ("WAYLAND_DISPLAY", "wayland-1"),
            ("A", "1"),
        ])
    );
    assert!(
        environment(panel, false, Some("t"), &vars)
            .contains(&("XDG_ACTIVATION_TOKEN".to_owned(), "t".to_owned()))
    );
}

#[test]
fn a_value_with_a_nul_is_dropped() {
    let vars = BTreeMap::from([("A".to_owned(), "a\0b".to_owned())]);
    assert!(environment(Vec::new(), true, None, &vars).is_empty());
}

#[test]
fn only_a_job_that_finished_otherwise_than_done_is_an_error() {
    assert!(finished(Some("done")).is_ok());
    assert!(finished(None).is_ok(), "an unanswered job was still queued");
    for result in ["failed", "timeout", "canceled", "dependency", "skipped"] {
        let error = finished(Some(result)).expect_err(result);
        assert!(error.message().contains(result), "{error}");
    }
}
