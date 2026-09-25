use std::io::Write;
use std::process::ExitCode;

use glimpse_shot::doc::wire::{ErrorKind, ErrorV1};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    Ok = 0,
    Failed = 1,
    Usage = 2,
    Config = 3,
    Cancelled = 4,
    NotFound = 5,
    TimedOut = 7,
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        Self::from(exit as u8)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Usage(pub String);

#[derive(Debug, thiserror::Error)]
#[error("cancelled")]
pub struct Cancelled;

#[derive(Debug, thiserror::Error)]
#[error("{what}")]
pub struct NotFound {
    pub what: String,
    pub hint: Option<&'static str>,
}

#[derive(Debug, thiserror::Error)]
#[error("timed out with no answer")]
pub struct TimedOut;

#[derive(Debug, thiserror::Error)]
#[error("the output was unplugged while the editor was open")]
pub struct OutputGone;

#[derive(Debug)]
pub struct Busy;

impl std::fmt::Display for Busy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&gettextrs::gettext("a screenshot is already being taken"))
    }
}

impl std::error::Error for Busy {}

pub fn exit(error: &anyhow::Error) -> Exit {
    if error.downcast_ref::<Usage>().is_some() {
        Exit::Usage
    } else if error
        .downcast_ref::<glimpse_config::ConfigError>()
        .is_some()
    {
        Exit::Config
    } else if error.downcast_ref::<Cancelled>().is_some() {
        Exit::Cancelled
    } else if error.downcast_ref::<NotFound>().is_some() {
        Exit::NotFound
    } else if error.downcast_ref::<TimedOut>().is_some() {
        Exit::TimedOut
    } else {
        Exit::Failed
    }
}

pub fn report(error: &anyhow::Error, json: bool, stderr: &mut impl Write) -> Exit {
    let code = exit(error);
    let hint = error
        .downcast_ref::<NotFound>()
        .and_then(|found| found.hint);
    let _ = writeln!(stderr, "glimpse-shot: error: {error:#}");
    if let Some(hint) = hint {
        let _ = writeln!(stderr, "hint: {hint}");
    }
    if json {
        let kind = if error.is::<OutputGone>() {
            ErrorKind::OutputGone
        } else if error.is::<Busy>() {
            ErrorKind::Busy
        } else {
            kind(code)
        };
        json_line(code, kind, format!("{error:#}"), hint, stderr);
    }
    code
}

pub fn clap_failure(error: &clap::Error, json: bool, stderr: &mut impl Write) -> Exit {
    let rendered = error.render().to_string();
    let _ = write!(stderr, "{rendered}");
    let message = rendered
        .lines()
        .next()
        .unwrap_or_default()
        .trim_start_matches("error: ")
        .to_owned();
    if json {
        json_line(Exit::Usage, ErrorKind::Usage, message, None, stderr);
    }
    Exit::Usage
}

fn json_line(
    code: Exit,
    kind: ErrorKind,
    message: String,
    hint: Option<&str>,
    stderr: &mut impl Write,
) {
    let line = ErrorV1 {
        code: code as u8,
        kind,
        message,
        hint: hint.map(str::to_owned),
    };
    if let Ok(text) = serde_json::to_string(&serde_json::json!({ "error": line })) {
        let _ = writeln!(stderr, "{text}");
    }
}

fn kind(code: Exit) -> ErrorKind {
    match code {
        Exit::Ok | Exit::Failed => ErrorKind::Failed,
        Exit::Usage => ErrorKind::Usage,
        Exit::Config => ErrorKind::Config,
        Exit::Cancelled => ErrorKind::Cancelled,
        Exit::NotFound => ErrorKind::TargetNotFound,
        Exit::TimedOut => ErrorKind::TimedOut,
    }
}

#[cfg(test)]
mod tests {
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
        let error = glimpse_config::load(Some(&path)).expect_err("an invalid template");

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
        assert!(
            text.starts_with("glimpse-shot: error: capturing: there is no output named DP-9\n")
        );
    }
}
