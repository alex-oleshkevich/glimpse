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
mod tests;
