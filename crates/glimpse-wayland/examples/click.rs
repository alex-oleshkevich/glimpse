use std::process::ExitCode;
use std::time::Duration;

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_output, wl_pointer, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, delegate_noop};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
};

const USAGE: &str = "usage: click OUTPUT X Y WIDTH HEIGHT left|right|middle|move";

#[derive(Default)]
struct State {
    outputs: Vec<(wl_output::WlOutput, String)>,
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

impl Dispatch<wl_output::WlOutput, ()> for State {
    fn event(
        state: &mut Self,
        output: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Name { name } = event {
            state.outputs.push((output.clone(), name));
        }
    }
}

delegate_noop!(State: ignore wl_seat::WlSeat);
delegate_noop!(State: ZwlrVirtualPointerManagerV1);
delegate_noop!(State: ZwlrVirtualPointerV1);

fn main() -> ExitCode {
    match run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("click: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let [output, x, y, width, height, action] = args else {
        return Err(USAGE.to_owned());
    };
    let number = |value: &str| {
        value
            .parse::<u32>()
            .map_err(|_| format!("{value} is not a coordinate"))
    };
    let (x, y, width, height) = (number(x)?, number(y)?, number(width)?, number(height)?);
    let button = match action.as_str() {
        "left" => Some(0x110),
        "right" => Some(0x111),
        "middle" => Some(0x112),
        "move" => None,
        _ => return Err(USAGE.to_owned()),
    };

    let connection = Connection::connect_to_env().map_err(|error| error.to_string())?;
    let (globals, mut queue) =
        registry_queue_init::<State>(&connection).map_err(|error| error.to_string())?;
    let handle = queue.handle();
    let manager: ZwlrVirtualPointerManagerV1 = globals
        .bind(&handle, 2..=2, ())
        .map_err(|_| "the compositor offers no zwlr_virtual_pointer_manager_v1 version 2")?;
    let seat: wl_seat::WlSeat = globals
        .bind(&handle, 1..=7, ())
        .map_err(|_| "the compositor offers no wl_seat")?;
    for global in globals.contents().clone_list() {
        if global.interface == "wl_output" && global.version >= 4 {
            globals
                .registry()
                .bind::<wl_output::WlOutput, _, _>(global.name, 4, &handle, ());
        }
    }
    let mut state = State::default();
    sync(&mut queue, &mut state)?;
    let target = state
        .outputs
        .iter()
        .find(|(_, name)| name == output)
        .map(|(target, _)| target)
        .ok_or_else(|| format!("no output named {output}"))?;

    let pointer =
        manager.create_virtual_pointer_with_output(Some(&seat), Some(target), &handle, ());
    sync(&mut queue, &mut state)?;
    pointer.motion_absolute(0, x, y, width, height);
    pointer.frame();
    sync(&mut queue, &mut state)?;
    if let Some(button) = button {
        pointer.button(10, button, wl_pointer::ButtonState::Pressed);
        pointer.frame();
        sync(&mut queue, &mut state)?;
        std::thread::sleep(Duration::from_millis(60));
        pointer.button(70, button, wl_pointer::ButtonState::Released);
        pointer.frame();
    }
    pointer.destroy();
    sync(&mut queue, &mut state)
}

fn sync(queue: &mut EventQueue<State>, state: &mut State) -> Result<(), String> {
    queue
        .roundtrip(state)
        .map(drop)
        .map_err(|error| error.to_string())
}
