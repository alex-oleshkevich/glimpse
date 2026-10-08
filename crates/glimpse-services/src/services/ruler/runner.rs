use std::future::Future;
use std::pin::Pin;
use std::process::Stdio;

use serde::Deserialize;

use super::history::Measurement;

const REASON: usize = 240;
const CANCELLED: i32 = 4;

pub type Measured = Pin<Box<dyn Future<Output = Result<Vec<Measurement>, String>> + Send>>;

pub trait Runner: Send + Sync + 'static {
    fn measure(&self) -> Measured;
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
    fn measure(&self) -> Measured {
        let mut command = tokio::process::Command::new(&self.program);
        command
            .args(["--tool", "ruler"])
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
mod tests;
