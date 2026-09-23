use std::future::Future;
use std::pin::Pin;
use std::process::Stdio;

use glimpse_config::ColorFormat;
use serde::Deserialize;

const REASON: usize = 240;
const CANCELLED: i32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    pub format: ColorFormat,
    pub lens_radius: u32,
    pub max_zoom: u32,
}

pub type Picked = Pin<Box<dyn Future<Output = Result<Option<[u8; 3]>, String>> + Send>>;

pub trait Picker: Send + Sync + 'static {
    fn pick(&self, request: Request) -> Picked;
}

pub struct ProcessPicker {
    program: String,
}

impl ProcessPicker {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
        }
    }
}

#[derive(Deserialize)]
struct Answer {
    red: u8,
    green: u8,
    blue: u8,
}

impl Picker for ProcessPicker {
    fn pick(&self, request: Request) -> Picked {
        let mut command = tokio::process::Command::new(&self.program);
        command
            .arg("--json")
            .args(["--format", request.format.name()])
            .args(["--lens-radius", &request.lens_radius.to_string()])
            .args(["--max-zoom", &request.max_zoom.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let program = self.program.clone();
        Box::pin(async move {
            let output = command
                .output()
                .await
                .map_err(|error| format!("cannot run {program}: {error}"))?;
            answer(output.status.code(), &output.stdout, &output.stderr)
        })
    }
}

fn answer(code: Option<i32>, stdout: &[u8], stderr: &[u8]) -> Result<Option<[u8; 3]>, String> {
    match code {
        Some(0) => {
            let answer: Answer = serde_json::from_slice(stdout)
                .map_err(|error| format!("the picker printed something unreadable: {error}"))?;
            Ok(Some([answer.red, answer.green, answer.blue]))
        }
        Some(CANCELLED) => Ok(None),
        _ => {
            let reason = String::from_utf8_lossy(stderr);
            let reason = glimpse_utils::clean(reason.trim(), REASON);
            Err(match reason.is_empty() {
                true => "the picker failed".to_owned(),
                false => reason,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pick_is_read_from_the_json_line() {
        let stdout = br##"{"format":"hex","value":"#E0563F","red":224,"green":86,"blue":63}"##;

        assert_eq!(answer(Some(0), stdout, b""), Ok(Some([224, 86, 63])));
    }

    #[test]
    fn a_cancel_is_no_color_and_no_error() {
        assert_eq!(answer(Some(4), b"", b"cancelled"), Ok(None));
    }

    #[test]
    fn a_failure_carries_what_the_picker_said() {
        let stderr = b"glimpse-picker: cannot capture the screen: no screencopy\n";

        assert_eq!(
            answer(Some(1), b"", stderr),
            Err("glimpse-picker: cannot capture the screen: no screencopy".to_owned())
        );
        assert_eq!(answer(None, b"", b""), Err("the picker failed".to_owned()));
    }

    #[test]
    fn unreadable_output_is_a_failure_not_a_color() {
        assert!(answer(Some(0), b"#E0563F", b"").is_err());
    }
}
