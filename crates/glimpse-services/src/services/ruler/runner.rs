use std::future::Future;
use std::pin::Pin;
use std::process::Stdio;

use serde::Deserialize;

use super::history::Measurement;

const REASON: usize = 240;
const CANCELLED: i32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    pub lens_radius: u32,
    pub max_zoom: u32,
}

pub type Measured = Pin<Box<dyn Future<Output = Result<Vec<Measurement>, String>> + Send>>;

pub trait Runner: Send + Sync + 'static {
    fn measure(&self, request: Request) -> Measured;
}

pub struct ProcessRunner {
    program: String,
}

impl ProcessRunner {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
        }
    }
}

#[derive(Deserialize)]
struct Line {
    from_x: u32,
    from_y: u32,
    to_x: u32,
    to_y: u32,
    dx: i64,
    dy: i64,
    distance: f64,
    angle: f64,
}

impl From<Line> for Measurement {
    fn from(line: Line) -> Self {
        Self {
            id: 0,
            from_x: line.from_x,
            from_y: line.from_y,
            to_x: line.to_x,
            to_y: line.to_y,
            dx: line.dx,
            dy: line.dy,
            distance: line.distance,
            angle: line.angle,
        }
    }
}

impl Runner for ProcessRunner {
    fn measure(&self, request: Request) -> Measured {
        let mut command = tokio::process::Command::new(&self.program);
        command
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

fn answer(code: Option<i32>, stdout: &[u8], stderr: &[u8]) -> Result<Vec<Measurement>, String> {
    match code {
        Some(0) => stdout
            .split(|&byte| byte == b'\n')
            .filter(|segment| !segment.is_empty())
            .map(|segment| {
                serde_json::from_slice::<Line>(segment)
                    .map(Measurement::from)
                    .map_err(|error| format!("the ruler printed something unreadable: {error}"))
            })
            .collect(),
        Some(CANCELLED) => Ok(Vec::new()),
        _ => {
            let reason = String::from_utf8_lossy(stderr);
            let reason = glimpse_utils::clean(reason.trim(), REASON);
            Err(match reason.is_empty() {
                true => "the ruler failed".to_owned(),
                false => reason,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(distance: f64) -> String {
        format!(
            r##"{{"from_x":1,"from_y":2,"to_x":3,"to_y":4,"dx":2,"dy":2,"distance":{distance},"angle":45.0}}"##
        )
    }

    #[test]
    fn every_ndjson_line_is_read_in_order() {
        let stdout = format!("{}\n{}\n", line(1.0), line(2.0));

        let measurements = answer(Some(0), stdout.as_bytes(), b"").unwrap();

        assert_eq!(measurements.len(), 2);
        assert_eq!(measurements[0].distance, 1.0);
        assert_eq!(measurements[1].distance, 2.0);
    }

    #[test]
    fn no_lines_on_exit_zero_is_an_empty_batch() {
        assert_eq!(answer(Some(0), b"", b""), Ok(Vec::new()));
    }

    #[test]
    fn a_cancel_is_no_measurements_and_no_error() {
        assert_eq!(answer(Some(4), b"", b"cancelled"), Ok(Vec::new()));
    }

    #[test]
    fn a_failure_carries_what_the_ruler_said() {
        let stderr = b"glimpse-ruler: cannot capture the screen: no screencopy\n";

        assert_eq!(
            answer(Some(1), b"", stderr),
            Err("glimpse-ruler: cannot capture the screen: no screencopy".to_owned())
        );
        assert_eq!(answer(None, b"", b""), Err("the ruler failed".to_owned()));
    }

    #[test]
    fn unreadable_output_is_a_failure_not_a_batch() {
        assert!(answer(Some(0), b"not json", b"").is_err());
    }

    #[test]
    fn one_bad_line_fails_the_whole_batch() {
        let stdout = format!("{}\nnot json\n", line(1.0));

        assert!(answer(Some(0), stdout.as_bytes(), b"").is_err());
    }
}
