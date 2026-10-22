mod ext;
mod wlr;

use std::fs::File;
use std::os::fd::AsFd;
use std::time::Instant;

use rustix::fs::{MemfdFlags, memfd_create};
use wayland_client::protocol::{wl_buffer, wl_callback, wl_output, wl_shm, wl_shm_pool};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum, delegate_dispatch};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_list_v1::ExtForeignToplevelListV1;
use wayland_protocols::ext::image_capture_source::v1::client::{
    ext_foreign_toplevel_image_capture_source_manager_v1::ExtForeignToplevelImageCaptureSourceManagerV1,
    ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
};
use wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_manager_v1::ExtImageCopyCaptureManagerV1;
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1;

use crate::wire::{self, Globals, Stall, Synced};

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error(
        "the compositor offers neither ext_image_copy_capture_manager_v1 nor zwlr_screencopy_manager_v1 for this capture"
    )]
    Unsupported,
    #[error("the compositor offered a {0} buffer, which cannot be read")]
    Format(String),
    #[error("the compositor refused to capture {0}")]
    Refused(String),
    #[error("there is no output to capture")]
    NoOutputs,
    #[error("there is no output named {0}")]
    NoSuchOutput(String),
    #[error("the compositor did not answer in time")]
    TimedOut,
    #[error("the compositor offers wl_output version {0}, and outputs have names only from 4")]
    Unnamed(u32),
    #[error("{0}")]
    Failed(String),
    #[error("there is no window with that identifier")]
    NoSuchToplevel,
}

fn failed(error: impl std::fmt::Display) -> CaptureError {
    CaptureError::Failed(error.to_string())
}

impl From<Stall> for CaptureError {
    fn from(stall: Stall) -> Self {
        match stall {
            Stall::TimedOut => Self::TimedOut,
            Stall::Failed(reason) => Self::Failed(reason),
        }
    }
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

#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub connector: String,
    pub width: u32,
    pub height: u32,
    pub pixels: glib::Bytes,
    pub layout: Layout,
    pub alpha: bool,
}

impl Frame {
    fn captured(connector: String, raw: Raw, bytes: Vec<u8>) -> Self {
        if raw.transform == wl_output::Transform::Normal
            && !raw.y_invert
            && raw.width.checked_mul(4) == Some(raw.stride)
            && bytes.len() == raw.stride as usize * raw.height as usize
        {
            return Self {
                connector,
                width: raw.width,
                height: raw.height,
                pixels: glib::Bytes::from_owned(bytes),
                layout: raw.layout,
                alpha: false,
            };
        }
        Self::upright(connector, raw, &bytes)
    }

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
                for (to, from) in target
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(source.as_chunks::<4>().0.iter())
                {
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
            layout: Layout::Rgbx,
            alpha: false,
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let x = x.min(self.width.saturating_sub(1)) as usize;
        let y = y.min(self.height.saturating_sub(1)) as usize;
        let at = (y * self.width as usize + x) * 4;
        match self.layout {
            Layout::Bgrx => [self.pixels[at + 2], self.pixels[at + 1], self.pixels[at]],
            Layout::Rgbx => [self.pixels[at], self.pixels[at + 1], self.pixels[at + 2]],
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Toplevel {
    pub identifier: String,
    pub app_id: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    ExtImageCopy,
    WlrScreencopy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub outputs: Backend,
    pub toplevels: bool,
}

pub fn capabilities(deadline: Instant) -> Result<Capabilities, CaptureError> {
    let (_connection, globals) = wire::connect(deadline)?;
    choose(&globals.pairs())
}

pub fn choose(globals: &[(&str, u32)]) -> Result<Capabilities, CaptureError> {
    let offers = |wanted: &str| globals.iter().any(|(interface, _)| *interface == wanted);
    let copy = offers(ExtImageCopyCaptureManagerV1::interface().name);
    let outputs = if copy && offers(ExtOutputImageCaptureSourceManagerV1::interface().name) {
        Backend::ExtImageCopy
    } else if offers(ZwlrScreencopyManagerV1::interface().name) {
        Backend::WlrScreencopy
    } else {
        return Err(CaptureError::Unsupported);
    };
    let toplevels = copy
        && offers(ExtForeignToplevelImageCaptureSourceManagerV1::interface().name)
        && offers(ExtForeignToplevelListV1::interface().name);
    Ok(Capabilities { outputs, toplevels })
}

pub fn outputs(deadline: Instant, cursor: bool) -> Result<Vec<Frame>, CaptureError> {
    shoot(deadline, cursor, None)
}

pub fn output(name: &str, deadline: Instant, cursor: bool) -> Result<Frame, CaptureError> {
    shoot(deadline, cursor, Some(name))?
        .into_iter()
        .next()
        .ok_or(CaptureError::NoOutputs)
}

pub fn toplevels(deadline: Instant) -> Result<Vec<Toplevel>, CaptureError> {
    let (connection, globals) = wire::connect(deadline)?;
    ext::toplevels(&connection, &globals, deadline)
}

pub fn toplevel(identifier: &str, deadline: Instant) -> Result<Frame, CaptureError> {
    let (connection, globals) = wire::connect(deadline)?;
    if !choose(&globals.pairs())?.toplevels {
        return Err(CaptureError::Unsupported);
    }
    ext::toplevel(&connection, &globals, identifier, deadline)
}

fn shoot(deadline: Instant, cursor: bool, only: Option<&str>) -> Result<Vec<Frame>, CaptureError> {
    let (connection, globals) = wire::connect(deadline)?;
    let backend = choose(&globals.pairs())?.outputs;
    let named = named(&connection, &globals, deadline, only)?;
    let frames = match backend {
        Backend::ExtImageCopy => ext::outputs(&connection, &globals, &named, deadline, cursor),
        Backend::WlrScreencopy => wlr::outputs(&connection, &globals, &named, deadline, cursor),
    };
    for output in &named {
        output.output.release();
    }
    connection.flush().map_err(failed)?;
    frames
}

struct Named {
    output: wl_output::WlOutput,
    connector: String,
    transform: wl_output::Transform,
}

#[derive(Default)]
struct Naming {
    outputs: Vec<(wl_output::WlOutput, Option<String>, wl_output::Transform)>,
}

fn named(
    connection: &Connection,
    globals: &Globals,
    deadline: Instant,
    only: Option<&str>,
) -> Result<Vec<Named>, CaptureError> {
    let mut queue = connection.new_event_queue::<Naming>();
    let handle = queue.handle();
    let mut naming = Naming::default();
    for global in globals.all(wl_output::WlOutput::interface().name) {
        if global.version < 4 {
            return Err(CaptureError::Unnamed(global.version));
        }
        let index = naming.outputs.len();
        let output =
            globals
                .registry()
                .bind::<wl_output::WlOutput, _, _>(global.name, 4, &handle, index);
        naming
            .outputs
            .push((output, None, wl_output::Transform::Normal));
    }
    if naming.outputs.is_empty() {
        return Err(CaptureError::NoOutputs);
    }
    wire::roundtrip(connection, &mut queue, &mut naming, deadline)?;
    naming.finish(only)
}

impl Naming {
    fn finish(self, only: Option<&str>) -> Result<Vec<Named>, CaptureError> {
        let mut named = Vec::new();
        for (output, connector, transform) in self.outputs {
            let Some(connector) = connector.filter(|name| only.is_none_or(|wanted| wanted == name))
            else {
                output.release();
                continue;
            };
            named.push(Named {
                output,
                connector,
                transform,
            });
        }
        match (named.is_empty(), only) {
            (true, Some(wanted)) => Err(CaptureError::NoSuchOutput(wanted.to_owned())),
            (true, None) => Err(CaptureError::NoOutputs),
            (false, _) => Ok(named),
        }
    }
}

delegate_dispatch!(Naming: [wl_callback::WlCallback: Synced] => Synced);

impl Dispatch<wl_output::WlOutput, usize> for Naming {
    fn event(
        state: &mut Self,
        _: &wl_output::WlOutput,
        event: wl_output::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some((_, connector, transform)) = state.outputs.get_mut(*index) else {
            return;
        };
        match event {
            wl_output::Event::Name { name } => *connector = Some(name),
            wl_output::Event::Geometry {
                transform: WEnum::Value(value),
                ..
            } => *transform = value,
            _ => {}
        }
    }
}

struct Memory {
    file: File,
    pool: wl_shm_pool::WlShmPool,
    buffer: wl_buffer::WlBuffer,
}

impl Memory {
    fn new<D>(
        shm: &wl_shm::WlShm,
        handle: &QueueHandle<D>,
        (width, height, stride): (u32, u32, u32),
        format: wl_shm::Format,
    ) -> Result<Self, CaptureError>
    where
        D: Dispatch<wl_shm_pool::WlShmPool, ()> + Dispatch<wl_buffer::WlBuffer, ()> + 'static,
    {
        let size = stride as usize * height as usize;
        let file =
            File::from(memfd_create("glimpse-capture", MemfdFlags::CLOEXEC).map_err(failed)?);
        file.set_len(size as u64).map_err(failed)?;
        let pool = shm.create_pool(file.as_fd(), size as i32, handle, ());
        let buffer = pool.create_buffer(
            0,
            width as i32,
            height as i32,
            stride as i32,
            format,
            handle,
            (),
        );
        Ok(Self { file, pool, buffer })
    }

    fn read(self, size: usize, ready: bool) -> Option<Vec<u8>> {
        use std::os::unix::fs::FileExt;
        self.buffer.destroy();
        self.pool.destroy();
        ready.then(|| {
            let mut bytes = vec![0u8; size];
            self.file.read_exact_at(&mut bytes, 0).map(|()| bytes).ok()
        })?
    }
}

#[cfg(test)]
mod tests;
