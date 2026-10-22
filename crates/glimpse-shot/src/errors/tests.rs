use anyhow::Context as _;

use super::*;

fn not_found() -> anyhow::Error {
    anyhow::Error::new(NotFound {
        what: "there is no output named DP-9".to_owned(),
        hint: Some("run `glimpse-shot list` to see the outputs"),
    })
}

#[test]
fn each_error_type_maps_to_its_code_through_two_contexts() {
    let wrapped = |error: anyhow::Error| {
        Err::<(), _>(error)
            .context("capturing")
            .context("taking a screenshot")
            .unwrap_err()
    };

    assert_eq!(exit(&wrapped(not_found())), Exit::NotFound);
    assert_eq!(
        exit(&wrapped(anyhow::Error::new(Usage("bad".to_owned())))),
        Exit::Usage
    );
    assert_eq!(
        exit(&wrapped(anyhow::Error::new(Cancelled))),
        Exit::Cancelled
    );
    assert_eq!(exit(&wrapped(anyhow::Error::new(TimedOut))), Exit::TimedOut);
    assert_eq!(exit(&wrapped(anyhow::anyhow!("disk full"))), Exit::Failed);
}

#[test]
fn a_configuration_error_maps_to_three() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[screenshot]\nfilename = \"no-extension\"\n").expect("writes");
    let error =
        glimpse_config::load_files(std::slice::from_ref(&path)).expect_err("an invalid template");

    assert_eq!(
        exit(&anyhow::Error::new(error).context("loading")),
        Exit::Config
    );
}

#[test]
fn a_clap_error_under_json_ends_stderr_with_a_usage_object() {
    let args = ["glimpse-shot", "capture", "--json", "--flash"];
    let error = <crate::cli::Cli as clap::Parser>::try_parse_from(args)
        .expect_err("--flash does not exist under capture");
    let mut stderr = Vec::new();

    let code = clap_failure(&error, true, &mut stderr);

    let text = String::from_utf8(stderr).expect("utf-8");
    let value: serde_json::Value =
        serde_json::from_str(text.lines().last().expect("a line")).expect("json");
    assert_eq!(code, Exit::Usage);
    assert_eq!(value["error"]["code"], 2);
    assert_eq!(value["error"]["kind"], "usage");
    assert!(text.contains("--flash"), "{text}");
}

#[test]
fn a_lost_output_and_a_second_invocation_fail_with_their_own_kind() {
    for (error, expected) in [
        (anyhow::Error::new(OutputGone), "output_gone"),
        (anyhow::Error::new(Busy), "busy"),
    ] {
        let mut stderr = Vec::new();
        let code = report(&error, true, &mut stderr);

        let text = String::from_utf8(stderr).expect("utf-8");
        let value: serde_json::Value =
            serde_json::from_str(text.lines().last().expect("a line")).expect("json");
        assert_eq!(code, Exit::Failed);
        assert_eq!(value["error"]["code"], 1);
        assert_eq!(value["error"]["kind"], expected);
    }
}

#[test]
fn with_json_the_last_stderr_line_is_the_error_object() {
    let mut stderr = Vec::new();
    let code = report(&not_found().context("capturing"), true, &mut stderr);

    let text = String::from_utf8(stderr).expect("utf-8");
    let last = text.lines().last().expect("a line");
    let value: serde_json::Value = serde_json::from_str(last).expect("json");
    assert_eq!(code, Exit::NotFound);
    assert_eq!(value["error"]["code"], 5);
    assert_eq!(value["error"]["kind"], "target_not_found");
    assert_eq!(
        value["error"]["hint"],
        "run `glimpse-shot list` to see the outputs"
    );
    assert!(text.starts_with("glimpse-shot: error: capturing: there is no output named DP-9\n"));
}
