use std::time::Instant;

use wayland_client::protocol::{wl_buffer, wl_callback, wl_output, wl_shm, wl_shm_pool};
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, WEnum, delegate_dispatch, delegate_noop,
    event_created_child,
};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1},
    ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
};
use wayland_protocols::ext::image_capture_source::v1::client::{
    ext_foreign_toplevel_image_capture_source_manager_v1::ExtForeignToplevelImageCaptureSourceManagerV1,
    ext_image_capture_source_v1::ExtImageCaptureSourceV1,
    ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
};
use wayland_protocols::ext::image_copy_capture::v1::client::{
    ext_image_copy_capture_frame_v1::{self, ExtImageCopyCaptureFrameV1},
    ext_image_copy_capture_manager_v1::{ExtImageCopyCaptureManagerV1, Options},
    ext_image_copy_capture_session_v1::{self, ExtImageCopyCaptureSessionV1},
};

use super::{CaptureError, Frame, Layout, Memory, Named, Raw, Toplevel, failed};
use crate::wire::{Globals, Synced, roundtrip, settle};

const APP_ID_CAP: usize = 128;
const TITLE_CAP: usize = 512;

struct Target {
    connector: String,
    source: ExtImageCaptureSourceV1,
    session: ExtImageCopyCaptureSessionV1,
    formats: Vec<wl_shm::Format>,
    size: Option<(u32, u32)>,
    constrained: bool,
    stopped: bool,
    frame: Option<ExtImageCopyCaptureFrameV1>,
    transform: wl_output::Transform,
    outcome: Option<bool>,
}

#[derive(Default)]
struct State {
    targets: Vec<Target>,
    toplevels: Vec<(ExtForeignToplevelHandleV1, Toplevel)>,
    listed: bool,
}

impl State {
    fn watch(
        &mut self,
        connector: String,
        source: ExtImageCaptureSourceV1,
        session: ExtImageCopyCaptureSessionV1,
    ) {
        self.targets.push(Target {
            connector,
            source,
            session,
            formats: Vec::new(),
            size: None,
            constrained: false,
            stopped: false,
            frame: None,
            transform: wl_output::Transform::Normal,
            outcome: None,
        });
    }
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
    let sources: ExtOutputImageCaptureSourceManagerV1 = globals
        .bind(&handle, 1..=1, ())
        .ok_or(CaptureError::Unsupported)?;
    let manager: ExtImageCopyCaptureManagerV1 = globals
        .bind(&handle, 1..=1, ())
        .ok_or(CaptureError::Unsupported)?;
    let options = match cursor {
        true => Options::PaintCursors,
        false => Options::empty(),
    };

    let mut state = State::default();
    for (index, output) in named.iter().enumerate() {
        let source = sources.create_source(&output.output, &handle, ());
        let session = manager.create_session(&source, options, &handle, index);
        state.watch(output.connector.clone(), source, session);
    }
    let frames = capture(connection, globals, &mut queue, &mut state, deadline);
    sources.destroy();
    manager.destroy();
    frames
}

pub(super) fn toplevels(
    connection: &Connection,
    globals: &Globals,
    deadline: Instant,
) -> Result<Vec<Toplevel>, CaptureError> {
    let mut queue = connection.new_event_queue::<State>();
    let mut state = State::default();
    let listed = list(connection, globals, &mut queue, &mut state, deadline);
    let toplevels = state
        .toplevels
        .drain(..)
        .map(|(handle, toplevel)| {
            handle.destroy();
            toplevel
        })
        .collect();
    connection.flush().map_err(failed)?;
    listed.map(|()| toplevels)
}

pub(super) fn toplevel(
    connection: &Connection,
    globals: &Globals,
    identifier: &str,
    deadline: Instant,
) -> Result<Frame, CaptureError> {
    let mut queue = connection.new_event_queue::<State>();
    let mut state = State::default();
    list(connection, globals, &mut queue, &mut state, deadline)?;

    let found = state
        .toplevels
        .iter()
        .find(|(_, known)| known.identifier == identifier)
        .map(|(toplevel, _)| toplevel.clone());
    let frames = match found {
        Some(toplevel) => capture_toplevel(
            connection, globals, &mut queue, &mut state, &toplevel, deadline,
        ),
        None => Err(CaptureError::NoSuchToplevel),
    };
    for (toplevel, _) in state.toplevels.drain(..) {
        toplevel.destroy();
    }
    frames?
        .into_iter()
        .next()
        .ok_or(CaptureError::NoSuchToplevel)
}

fn list(
    connection: &Connection,
    globals: &Globals,
    queue: &mut EventQueue<State>,
    state: &mut State,
    deadline: Instant,
) -> Result<(), CaptureError> {
    let handle = queue.handle();
    let list: ExtForeignToplevelListV1 = globals
        .bind(&handle, 1..=1, ())
        .ok_or(CaptureError::Unsupported)?;
    roundtrip(connection, queue, state, deadline)?;
    list.stop();
    let settled = settle(connection, queue, state, deadline, |state| state.listed);
    list.destroy();
    settled.map_err(CaptureError::from)
}

fn capture_toplevel(
    connection: &Connection,
    globals: &Globals,
    queue: &mut EventQueue<State>,
    state: &mut State,
    toplevel: &ExtForeignToplevelHandleV1,
    deadline: Instant,
) -> Result<Vec<Frame>, CaptureError> {
    let handle = queue.handle();
    let sources: ExtForeignToplevelImageCaptureSourceManagerV1 =
        globals
            .bind(&handle, 1..=1, ())
            .ok_or(CaptureError::Unsupported)?;
    let manager: ExtImageCopyCaptureManagerV1 = globals
        .bind(&handle, 1..=1, ())
        .ok_or(CaptureError::Unsupported)?;
    let source = sources.create_source(toplevel, &handle, ());
    let session = manager.create_session(&source, Options::empty(), &handle, 0);
    state.watch(String::new(), source, session);
    let frames = capture(connection, globals, queue, state, deadline);
    sources.destroy();
    manager.destroy();
    frames
}

fn readable(format: wl_shm::Format) -> bool {
    matches!(format, wl_shm::Format::Xrgb8888 | wl_shm::Format::Argb8888)
}

fn capture(
    connection: &Connection,
    globals: &Globals,
    queue: &mut EventQueue<State>,
    state: &mut State,
    deadline: Instant,
) -> Result<Vec<Frame>, CaptureError> {
    let handle = queue.handle();
    let shm: wl_shm::WlShm = globals
        .bind(&handle, 1..=1, ())
        .ok_or_else(|| failed("the compositor offers no wl_shm"))?;
    settle(connection, queue, state, deadline, |state| {
        state
            .targets
            .iter()
            .all(|target| target.constrained || target.stopped)
    })?;

    let mut memory = Vec::new();
    for (index, target) in state.targets.iter_mut().enumerate() {
        let refused = || CaptureError::Refused(target.connector.clone());
        let (width, height) = target
            .size
            .filter(|_| !target.stopped)
            .ok_or_else(refused)?;
        let Some(format) = target
            .formats
            .iter()
            .copied()
            .find(|format| readable(*format))
        else {
            return Err(CaptureError::Format(match target.formats.first() {
                Some(format) => format!("{format:?}"),
                None => "non-shm".to_owned(),
            }));
        };
        let stride = width * 4;
        let shared = Memory::new(&shm, &handle, (width, height, stride), format)?;
        let frame = target.session.create_frame(&handle, index);
        frame.attach_buffer(&shared.buffer);
        frame.damage_buffer(0, 0, width as i32, height as i32);
        frame.capture();
        target.frame = Some(frame);
        let raw = Raw {
            width,
            height,
            stride,
            layout: Layout::Bgrx,
            y_invert: false,
            transform: wl_output::Transform::Normal,
        };
        memory.push((shared, raw));
    }

    settle(connection, queue, state, deadline, |state| {
        state.targets.iter().all(|target| target.outcome.is_some())
    })?;

    let mut frames = Vec::new();
    for (target, (shared, raw)) in state.targets.iter().zip(memory) {
        if let Some(frame) = &target.frame {
            frame.destroy();
        }
        target.session.destroy();
        target.source.destroy();
        let size = raw.stride as usize * raw.height as usize;
        let Some(bytes) = shared.read(size, target.outcome == Some(true)) else {
            return Err(CaptureError::Refused(target.connector.clone()));
        };
        let raw = Raw {
            transform: target.transform,
            ..raw
        };
        tracing::debug!(
            connector = target.connector,
            ?raw,
            "image copy capture buffer"
        );
        frames.push(Frame::captured(target.connector.clone(), raw, bytes));
    }
    connection.flush().map_err(failed)?;
    Ok(frames)
}

impl Dispatch<ExtImageCopyCaptureSessionV1, usize> for State {
    fn event(
        state: &mut Self,
        _: &ExtImageCopyCaptureSessionV1,
        event: ext_image_copy_capture_session_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(target) = state.targets.get_mut(*index) else {
            return;
        };
        match event {
            ext_image_copy_capture_session_v1::Event::BufferSize { width, height } => {
                target.size = Some((width, height));
            }
            ext_image_copy_capture_session_v1::Event::ShmFormat {
                format: WEnum::Value(format),
            } => target.formats.push(format),
            ext_image_copy_capture_session_v1::Event::Done => target.constrained = true,
            ext_image_copy_capture_session_v1::Event::Stopped => target.stopped = true,
            _ => {}
        }
    }
}

impl Dispatch<ExtImageCopyCaptureFrameV1, usize> for State {
    fn event(
        state: &mut Self,
        _: &ExtImageCopyCaptureFrameV1,
        event: ext_image_copy_capture_frame_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(target) = state.targets.get_mut(*index) else {
            return;
        };
        match event {
            ext_image_copy_capture_frame_v1::Event::Transform {
                transform: WEnum::Value(transform),
            } => target.transform = transform,
            ext_image_copy_capture_frame_v1::Event::Ready => target.outcome = Some(true),
            ext_image_copy_capture_frame_v1::Event::Failed { .. } => target.outcome = Some(false),
            _ => {}
        }
    }
}

impl Dispatch<ExtForeignToplevelListV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ExtForeignToplevelListV1,
        event: ext_foreign_toplevel_list_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_foreign_toplevel_list_v1::Event::Toplevel { toplevel } => {
                state.toplevels.push((toplevel, Toplevel::default()));
            }
            ext_foreign_toplevel_list_v1::Event::Finished => state.listed = true,
            _ => {}
        }
    }

    event_created_child!(State, ExtForeignToplevelListV1, [
        ext_foreign_toplevel_list_v1::EVT_TOPLEVEL_OPCODE => (ExtForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ExtForeignToplevelHandleV1, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &ExtForeignToplevelHandleV1,
        event: ext_foreign_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some((_, known)) = state
            .toplevels
            .iter_mut()
            .find(|(toplevel, _)| toplevel == proxy)
        else {
            return;
        };
        match event {
            ext_foreign_toplevel_handle_v1::Event::Identifier { identifier } => {
                known.identifier = identifier;
            }
            ext_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                known.app_id = Some(app_id.chars().take(APP_ID_CAP).collect());
            }
            ext_foreign_toplevel_handle_v1::Event::Title { title } => {
                known.title = Some(title.chars().take(TITLE_CAP).collect());
            }
            _ => {}
        }
    }
}

delegate_dispatch!(State: [wl_callback::WlCallback: Synced] => Synced);
delegate_noop!(State: ignore wl_shm::WlShm);
delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
delegate_noop!(State: ignore wl_buffer::WlBuffer);
delegate_noop!(State: ignore ExtImageCopyCaptureManagerV1);
delegate_noop!(State: ignore ExtOutputImageCaptureSourceManagerV1);
delegate_noop!(State: ignore ExtForeignToplevelImageCaptureSourceManagerV1);
delegate_noop!(State: ignore ExtImageCaptureSourceV1);
