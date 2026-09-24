use std::fs::File;
use std::os::fd::AsFd;
use std::os::unix::fs::FileExt;
use std::time::Instant;

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::fs::{MemfdFlags, memfd_create};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_buffer, wl_output, wl_registry, wl_shm, wl_shm_pool};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum, delegate_noop};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1},
    zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1,
};

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("the compositor does not offer zwlr_screencopy_manager_v1")]
    Unsupported,
    #[error("the compositor offered a {0} buffer, which the picker cannot read")]
    Format(String),
    #[error("the compositor refused to capture {0}")]
    Refused(String),
    #[error("there is no output to capture")]
    NoOutputs,
    #[error("the compositor did not answer in time")]
    TimedOut,
    #[error("the compositor offers wl_output version {0}, and outputs have names only from 4")]
    Unnamed(u32),
    #[error("{0}")]
    Failed(String),
}

fn failed(error: impl std::fmt::Display) -> CaptureError {
    CaptureError::Failed(error.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Bgrx,
    Rgbx,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Raw {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub layout: Layout,
    pub y_invert: bool,
    pub transform: wl_output::Transform,
}

pub struct Frame {
    pub connector: String,
    pub width: u32,
    pub height: u32,
    pub pixels: glib::Bytes,
}

impl Frame {
    pub fn upright(connector: String, raw: Raw, bytes: &[u8]) -> Self {
        use wl_output::Transform as T;
        let (w, h) = (raw.width as usize, raw.height as usize);
        let turned = matches!(
            raw.transform,
            T::_90 | T::_270 | T::Flipped90 | T::Flipped270
        );
        let (width, height) = if turned { (h, w) } else { (w, h) };
        let mut pixels = vec![0u8; width * height * 4];
        let rows_only = match raw.transform {
            T::Normal => Some(raw.y_invert),
            T::Flipped180 => Some(!raw.y_invert),
            _ => None,
        };
        if let Some(inverted) = rows_only {
            for (y, target) in pixels.chunks_exact_mut(width * 4).enumerate() {
                let v = if inverted { h - 1 - y } else { y };
                let start = v * raw.stride as usize;
                let Some(source) = bytes.get(start..start + w * 4) else {
                    continue;
                };
                for (to, from) in target.chunks_exact_mut(4).zip(source.chunks_exact(4)) {
                    to.copy_from_slice(&match raw.layout {
                        Layout::Bgrx => [from[2], from[1], from[0], 255],
                        Layout::Rgbx => [from[0], from[1], from[2], 255],
                    });
                }
            }
            return Self::built(connector, width, height, pixels);
        }
        for y in 0..height {
            for x in 0..width {
                let (u, v) = match raw.transform {
                    T::_90 => (y, h - 1 - x),
                    T::_180 => (w - 1 - x, h - 1 - y),
                    T::_270 => (w - 1 - y, x),
                    T::Flipped => (w - 1 - x, y),
                    T::Flipped90 => (y, x),
                    T::Flipped180 => (x, h - 1 - y),
                    T::Flipped270 => (w - 1 - y, h - 1 - x),
                    _ => (x, y),
                };
                let v = if raw.y_invert { h - 1 - v } else { v };
                let at = v * raw.stride as usize + u * 4;
                let Some(source) = bytes.get(at..at + 4) else {
                    continue;
                };
                let rgb = match raw.layout {
                    Layout::Bgrx => [source[2], source[1], source[0]],
                    Layout::Rgbx => [source[0], source[1], source[2]],
                };
                let to = (y * width + x) * 4;
                pixels[to..to + 3].copy_from_slice(&rgb);
                pixels[to + 3] = 255;
            }
        }
        Self::built(connector, width, height, pixels)
    }

    fn built(connector: String, width: usize, height: usize, pixels: Vec<u8>) -> Self {
        Self {
            connector,
            width: width as u32,
            height: height as u32,
            pixels: glib::Bytes::from_owned(pixels),
        }
    }

    #[cfg(test)]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let x = x.min(self.width.saturating_sub(1)) as usize;
        let y = y.min(self.height.saturating_sub(1)) as usize;
        let at = (y * self.width as usize + x) * 4;
        [self.pixels[at], self.pixels[at + 1], self.pixels[at + 2]]
    }
}

struct Output {
    output: wl_output::WlOutput,
    connector: Option<String>,
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

pub fn capture(deadline: Instant) -> Result<Vec<Frame>, CaptureError> {
    let connection = Connection::connect_to_env().map_err(failed)?;
    let (globals, mut queue) = registry_queue_init::<State>(&connection).map_err(failed)?;
    let handle = queue.handle();
    let manager: ZwlrScreencopyManagerV1 = globals
        .bind(&handle, 1..=3, ())
        .map_err(|_| CaptureError::Unsupported)?;
    let shm: wl_shm::WlShm = globals.bind(&handle, 1..=1, ()).map_err(failed)?;

    let mut state = State::default();
    for global in globals.contents().clone_list() {
        if global.interface != "wl_output" {
            continue;
        }
        if global.version < 4 {
            return Err(CaptureError::Unnamed(global.version));
        }
        let index = state.outputs.len();
        let output = globals.registry().bind::<wl_output::WlOutput, _, _>(
            global.name,
            global.version.min(4),
            &handle,
            index,
        );
        state.outputs.push(Output {
            output,
            connector: None,
            transform: wl_output::Transform::Normal,
            frame: None,
            buffer: None,
            buffer_done: false,
            y_invert: false,
            outcome: None,
        });
    }
    if state.outputs.is_empty() {
        return Err(CaptureError::NoOutputs);
    }
    settle(&connection, &mut queue, &mut state, deadline, |state| {
        state
            .outputs
            .iter()
            .all(|output| output.connector.is_some())
    })?;

    for (index, output) in state.outputs.iter_mut().enumerate() {
        output.frame = Some(manager.capture_output(0, &output.output, &handle, index));
    }
    let announces_done = manager.version() >= 3;
    settle(&connection, &mut queue, &mut state, deadline, |state| {
        state.outputs.iter().all(|output| {
            output.outcome.is_some()
                || output.buffer_done
                || (!announces_done && output.buffer.is_some())
        })
    })?;

    let mut memory = Vec::new();
    for output in &state.outputs {
        let name = output.connector.clone().unwrap_or_default();
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
        let size = stride as usize * height as usize;
        let file = File::from(memfd_create("glimpse-picker", MemfdFlags::CLOEXEC).map_err(failed)?);
        file.set_len(size as u64).map_err(failed)?;
        let pool = shm.create_pool(file.as_fd(), size as i32, &handle, ());
        let buffer = pool.create_buffer(
            0,
            width as i32,
            height as i32,
            stride as i32,
            format,
            &handle,
            (),
        );
        if let Some(frame) = &output.frame {
            frame.copy(&buffer);
        }
        memory.push((file, pool, buffer, raw));
    }

    settle(&connection, &mut queue, &mut state, deadline, |state| {
        state.outputs.iter().all(|output| output.outcome.is_some())
    })?;

    let mut frames = Vec::new();
    for (output, (file, pool, buffer, raw)) in state.outputs.iter().zip(memory) {
        let name = output.connector.clone().unwrap_or_default();
        if let Some(frame) = &output.frame {
            frame.destroy();
        }
        let mut bytes = vec![0u8; raw.stride as usize * raw.height as usize];
        let read = output.outcome == Some(true) && file.read_exact_at(&mut bytes, 0).is_ok();
        buffer.destroy();
        pool.destroy();
        if !read {
            return Err(CaptureError::Refused(name));
        }
        let raw = Raw {
            y_invert: output.y_invert,
            ..raw
        };
        tracing::debug!(connector = name, ?raw, "screencopy buffer");
        frames.push(Frame::upright(name, raw, &bytes));
    }
    for output in &state.outputs {
        output.output.release();
    }
    manager.destroy();
    connection.flush().map_err(failed)?;
    Ok(frames)
}

fn settle(
    connection: &Connection,
    queue: &mut EventQueue<State>,
    state: &mut State,
    deadline: Instant,
    done: impl Fn(&State) -> bool,
) -> Result<(), CaptureError> {
    loop {
        queue.dispatch_pending(state).map_err(failed)?;
        if done(state) {
            return Ok(());
        }
        connection.flush().map_err(failed)?;
        let Some(guard) = queue.prepare_read() else {
            continue;
        };
        let left = deadline
            .checked_duration_since(Instant::now())
            .filter(|left| !left.is_zero())
            .ok_or(CaptureError::TimedOut)?;
        let timeout = Timespec {
            tv_sec: left.as_secs() as i64,
            tv_nsec: i64::from(left.subsec_nanos()),
        };
        let fd = guard.connection_fd();
        let mut fds = [PollFd::new(&fd, PollFlags::IN)];
        match poll(&mut fds, Some(&timeout)) {
            Ok(0) => return Err(CaptureError::TimedOut),
            Ok(_) => {
                guard.read().map_err(failed)?;
            }
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(failed(error)),
        }
    }
}

fn layout(format: wl_shm::Format) -> Option<Layout> {
    match format {
        wl_shm::Format::Xrgb8888 | wl_shm::Format::Argb8888 => Some(Layout::Bgrx),
        wl_shm::Format::Xbgr8888 | wl_shm::Format::Abgr8888 => Some(Layout::Rgbx),
        _ => None,
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_output::WlOutput, usize> for State {
    fn event(
        state: &mut Self,
        _: &wl_output::WlOutput,
        event: wl_output::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(output) = state.outputs.get_mut(*index) else {
            return;
        };
        match event {
            wl_output::Event::Name { name } => output.connector = Some(name),
            wl_output::Event::Geometry {
                transform: WEnum::Value(transform),
                ..
            } => output.transform = transform,
            _ => {}
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(width: u32, height: u32, transform: wl_output::Transform, y_invert: bool) -> Raw {
        Raw {
            width,
            height,
            stride: width * 4,
            layout: Layout::Rgbx,
            y_invert,
            transform,
        }
    }

    fn numbered(width: u32, height: u32) -> Vec<u8> {
        (0..width * height)
            .flat_map(|index| [index as u8, 0, 0, 255])
            .collect()
    }

    fn reds(frame: &Frame) -> Vec<u8> {
        frame.pixels.chunks(4).map(|pixel| pixel[0]).collect()
    }

    #[test]
    fn a_bgrx_buffer_reads_as_red_green_blue() {
        let bytes = [0x3F, 0x56, 0xE0, 0xFF];
        let raw = Raw {
            layout: Layout::Bgrx,
            ..raw(1, 1, wl_output::Transform::Normal, false)
        };
        let frame = Frame::upright("DP-1".to_owned(), raw, &bytes);

        assert_eq!(frame.pixel(0, 0), [0xE0, 0x56, 0x3F]);
    }

    #[test]
    fn a_normal_buffer_is_read_row_by_row() {
        let frame = Frame::upright(
            String::new(),
            raw(3, 2, wl_output::Transform::Normal, false),
            &numbered(3, 2),
        );

        assert_eq!((frame.width, frame.height), (3, 2));
        assert_eq!(reds(&frame), [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn an_inverted_buffer_is_read_bottom_row_first() {
        let frame = Frame::upright(
            String::new(),
            raw(3, 2, wl_output::Transform::Normal, true),
            &numbered(3, 2),
        );

        assert_eq!(reds(&frame), [3, 4, 5, 0, 1, 2]);
    }

    #[test]
    fn a_quarter_turn_swaps_the_sides() {
        let frame = Frame::upright(
            String::new(),
            raw(3, 2, wl_output::Transform::_90, false),
            &numbered(3, 2),
        );

        assert_eq!((frame.width, frame.height), (2, 3));
        assert_eq!(reds(&frame), [3, 0, 4, 1, 5, 2]);
    }

    #[test]
    fn a_half_turn_reverses_the_buffer() {
        let frame = Frame::upright(
            String::new(),
            raw(3, 2, wl_output::Transform::_180, false),
            &numbered(3, 2),
        );

        assert_eq!(reds(&frame), [5, 4, 3, 2, 1, 0]);
    }

    fn grim(raw: Raw, x: usize, y: usize) -> (usize, usize) {
        use wl_output::Transform as T;
        let (w, h) = (raw.width as f64, raw.height as f64);
        let (angle, flipped) = match raw.transform {
            T::Normal => (0.0, false),
            T::_90 => (std::f64::consts::FRAC_PI_2, false),
            T::_180 => (std::f64::consts::PI, false),
            T::_270 => (3.0 * std::f64::consts::FRAC_PI_2, false),
            T::Flipped => (0.0, true),
            T::Flipped90 => (std::f64::consts::FRAC_PI_2, true),
            T::Flipped180 => (std::f64::consts::PI, true),
            _ => (3.0 * std::f64::consts::FRAC_PI_2, true),
        };
        let (cos, sin) = (angle.cos().round(), angle.sin().round());
        let turned = sin != 0.0;
        let (lw, lh) = if turned { (h, w) } else { (w, h) };
        for v in 0..raw.height as usize {
            for u in 0..raw.width as usize {
                let a = u as f64 + 0.5 - w / 2.0;
                let mut b = v as f64 + 0.5 - h / 2.0;
                if raw.y_invert {
                    b = -b;
                }
                let (mut rx, ry) = (cos * a - sin * b, sin * a + cos * b);
                if flipped {
                    rx = -rx;
                }
                let (lx, ly) = (rx + lw / 2.0 - 0.5, ry + lh / 2.0 - 0.5);
                if lx.round() as usize == x && ly.round() as usize == y {
                    return (u, v);
                }
            }
        }
        unreachable!("every logical pixel has a source")
    }

    #[test]
    fn every_transform_matches_the_mapping_grim_uses() {
        use wl_output::Transform as T;
        let transforms = [
            T::Normal,
            T::_90,
            T::_180,
            T::_270,
            T::Flipped,
            T::Flipped90,
            T::Flipped180,
            T::Flipped270,
        ];
        for transform in transforms {
            for y_invert in [false, true] {
                let raw = raw(3, 2, transform, y_invert);
                let frame = Frame::upright(String::new(), raw, &numbered(3, 2));
                for y in 0..frame.height as usize {
                    for x in 0..frame.width as usize {
                        let (u, v) = grim(raw, x, y);
                        assert_eq!(
                            frame.pixel(x as u32, y as u32)[0] as usize,
                            v * 3 + u,
                            "{transform:?} y_invert={y_invert} at ({x}, {y})"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_pixel_past_the_edge_reads_the_edge() {
        let frame = Frame::upright(
            String::new(),
            raw(3, 2, wl_output::Transform::Normal, false),
            &numbered(3, 2),
        );

        assert_eq!(frame.pixel(99, 99), [5, 0, 0]);
    }
}
