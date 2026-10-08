use std::time::Instant;

use wayland_client::protocol::{wl_buffer, wl_output, wl_shm, wl_shm_pool};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum, delegate_noop};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1},
    zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1,
};

use super::{CaptureError, Frame, Layout, Memory, Named, Raw, failed};
use crate::wire::{Globals, settle};

struct Output {
    connector: String,
    transform: wl_output::Transform,
    frame: Option<ZwlrScreencopyFrameV1>,
    buffer: Option<(wl_shm::Format, u32, u32, u32)>,
    buffer_done: bool,
    y_invert: bool,
    outcome: Option<bool>,
}

#[derive(Default)]
struct State {
    outputs: Vec<Output>,
}

pub(super) fn outputs(
    connection: &Connection,
    globals: &Globals,
    named: &[Named],
    deadline: Instant,
    cursor: bool,
) -> Result<Vec<Frame>, CaptureError> {
    let mut queue = connection.new_event_queue::<State>();
    let handle = queue.handle();
    let manager: ZwlrScreencopyManagerV1 = globals
        .bind(&handle, 1..=3, ())
        .ok_or(CaptureError::Unsupported)?;
    let shm: wl_shm::WlShm = globals
        .bind(&handle, 1..=1, ())
        .ok_or_else(|| failed("the compositor offers no wl_shm"))?;

    let mut state = State::default();
    for (index, output) in named.iter().enumerate() {
        state.outputs.push(Output {
            connector: output.connector.clone(),
            transform: output.transform,
            frame: Some(manager.capture_output(i32::from(cursor), &output.output, &handle, index)),
            buffer: None,
            buffer_done: false,
            y_invert: false,
            outcome: None,
        });
    }
    let announces_done = manager.version() >= 3;
    settle(connection, &mut queue, &mut state, deadline, |state| {
        state.outputs.iter().all(|output| {
            output.outcome.is_some()
                || output.buffer_done
                || (!announces_done && output.buffer.is_some())
        })
    })?;

    let mut memory = Vec::new();
    for output in &state.outputs {
        let name = output.connector.clone();
        let Some((format, width, height, stride)) = output.buffer else {
            return Err(CaptureError::Refused(name));
        };
        let Some(layout) = layout(format) else {
            return Err(CaptureError::Format(format!("{format:?}")));
        };
        let raw = Raw {
            width,
            height,
            stride,
            layout,
            y_invert: false,
            transform: output.transform,
        };
        let shared = Memory::new(&shm, &handle, (width, height, stride), format)?;
        if let Some(frame) = &output.frame {
            frame.copy(&shared.buffer);
        }
        memory.push((shared, raw));
    }

    settle(connection, &mut queue, &mut state, deadline, |state| {
        state.outputs.iter().all(|output| output.outcome.is_some())
    })?;

    let mut frames = Vec::new();
    for (output, (shared, raw)) in state.outputs.iter().zip(memory) {
        let name = output.connector.clone();
        if let Some(frame) = &output.frame {
            frame.destroy();
        }
        let size = raw.stride as usize * raw.height as usize;
        let Some(bytes) = shared.read(size, output.outcome == Some(true)) else {
            return Err(CaptureError::Refused(name));
        };
        let raw = Raw {
            y_invert: output.y_invert,
            ..raw
        };
        tracing::debug!(connector = name, ?raw, "screencopy buffer");
        frames.push(Frame::captured(name, raw, bytes));
    }
    manager.destroy();
    connection.flush().map_err(failed)?;
    Ok(frames)
}

fn layout(format: wl_shm::Format) -> Option<Layout> {
    match format {
        wl_shm::Format::Xrgb8888 | wl_shm::Format::Argb8888 => Some(Layout::Bgrx),
        wl_shm::Format::Xbgr8888 | wl_shm::Format::Abgr8888 => Some(Layout::Rgbx),
        _ => None,
    }
}

impl Dispatch<ZwlrScreencopyFrameV1, usize> for State {
    fn event(
        state: &mut Self,
        _: &ZwlrScreencopyFrameV1,
        event: zwlr_screencopy_frame_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(output) = state.outputs.get_mut(*index) else {
            return;
        };
        match event {
            zwlr_screencopy_frame_v1::Event::Buffer {
                format: WEnum::Value(format),
                width,
                height,
                stride,
            } => {
                let preferred = output
                    .buffer
                    .is_some_and(|(chosen, ..)| layout(chosen).is_some());
                if !preferred {
                    output.buffer = Some((format, width, height, stride));
                }
            }
            zwlr_screencopy_frame_v1::Event::Flags {
                flags: WEnum::Value(flags),
            } => {
                output.y_invert = flags.contains(zwlr_screencopy_frame_v1::Flags::YInvert);
            }
            zwlr_screencopy_frame_v1::Event::BufferDone => output.buffer_done = true,
            zwlr_screencopy_frame_v1::Event::Ready { .. } => output.outcome = Some(true),
            zwlr_screencopy_frame_v1::Event::Failed => output.outcome = Some(false),
            _ => {}
        }
    }
}

delegate_noop!(State: ignore wl_shm::WlShm);
delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
delegate_noop!(State: ignore wl_buffer::WlBuffer);
delegate_noop!(State: ignore ZwlrScreencopyManagerV1);
