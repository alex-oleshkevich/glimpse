use super::*;

#[test]
fn transient_names_escape_program_basenames() {
    for (program, escaped) in [
        ("/usr/bin/a-b.c é", "a\\x2db.c\\x20\\xc3\\xa9"),
        (".a_:@\\", "\\x2ea_:\\x40\\x5c"),
    ] {
        let (unit, _) = transient_unit(
            &[program.to_owned()],
            "/resolved",
            "42-7",
            [None, None, None],
        );
        assert_eq!(unit, format!("app-glimpse-{escaped}-42-7.service"));
    }
}

#[test]
fn transient_properties_preserve_argv_and_use_the_resolved_executable() {
    let command = vec!["a-b".to_owned(), "two words".to_owned(), "$HOME".to_owned()];
    let (_, properties) = transient_unit(&command, "/usr/bin/a-b", "42-7", [None, None, None]);
    assert_eq!(
        properties,
        vec![
            (
                "ExecStart",
                Value::from(vec![("/usr/bin/a-b".to_owned(), command, false)])
            ),
            ("Type", Value::from("exec")),
            ("ExitType", Value::from("cgroup")),
            ("Slice", Value::from("app.slice")),
            ("CollectMode", Value::from("inactive-or-failed")),
            ("Environment", Value::from(Vec::<String>::new())),
        ]
    );
    assert_eq!(properties[0].1.value_signature().to_string(), "a(sasb)");
}

#[test]
fn transient_environment_includes_only_set_launch_variables() {
    for (environment, expected) in [
        ([None, None, None], vec![]),
        (
            [Some("token"), Some("wayland-1"), Some(":0")],
            vec![
                "XDG_ACTIVATION_TOKEN=token",
                "WAYLAND_DISPLAY=wayland-1",
                "DISPLAY=:0",
            ],
        ),
        (
            [None, Some("wayland-1"), None],
            vec!["WAYLAND_DISPLAY=wayland-1"],
        ),
        (
            [Some(""), None, Some(":1")],
            vec!["XDG_ACTIVATION_TOKEN=", "DISPLAY=:1"],
        ),
    ] {
        let (_, properties) = transient_unit(
            &["program".to_owned()],
            "/resolved",
            "42-7",
            environment.map(|value| value.map(str::to_owned)),
        );
        let value = &properties
            .iter()
            .find(|(name, _)| *name == "Environment")
            .expect("environment property")
            .1;
        assert_eq!(value, &Value::from(expected));
        assert_eq!(value.value_signature().to_string(), "as");
    }
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
