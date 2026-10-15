use std::cell::RefCell;
use std::time::{Duration, Instant};

use glimpse_compositors::{Compositor, Snapshot, detect_compositor};
use glimpse_shot::ports::Screens;
use glimpse_wayland::capture::{self, CaptureError, Frame};

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(2);

pub fn compositor_snapshot() -> anyhow::Result<Option<Snapshot>> {
    let compositor = detect_compositor();
    if matches!(compositor, Compositor::Unsupported) {
        return Ok(None);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    Ok(Some(runtime.block_on(compositor.snapshot())?))
}

pub struct WaylandScreens {
    pub compositor: bool,
    pub counted: RefCell<Option<Frame>>,
}

impl Screens for WaylandScreens {
    fn current(&self) -> Result<String, CaptureError> {
        if self.compositor {
            return compositor_snapshot()
                .map_err(|error| CaptureError::Failed(format!("{error:#}")))?
                .and_then(|snapshot| snapshot.focused_output)
                .ok_or_else(|| CaptureError::NoSuchOutput("current".to_owned()));
        }
        let frames = capture::outputs(Instant::now() + CAPTURE_TIMEOUT, false)?;
        match <[Frame; 1]>::try_from(frames) {
            Ok([only]) => {
                let name = only.connector.clone();
                self.counted.replace(Some(only));
                Ok(name)
            }
            Err(_) => Err(CaptureError::NoSuchOutput("current".to_owned())),
        }
    }

    fn capture(&self, name: &str, cursor: bool) -> Result<Frame, CaptureError> {
        match self.counted.take() {
            Some(frame) if !cursor && frame.connector == name => Ok(frame),
            _ => capture::output(name, Instant::now() + CAPTURE_TIMEOUT, cursor),
        }
    }
}
