use std::os::unix::net::UnixStream;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use glimpse_compositors::contract::Error;
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use wayland_client::protocol::{wl_callback, wl_registry};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_client::{delegate_noop, event_created_child};
use wayland_protocols_wlr::output_management::v1::client::{
    zwlr_output_configuration_head_v1::ZwlrOutputConfigurationHeadV1,
    zwlr_output_configuration_v1::{self, ZwlrOutputConfigurationV1},
    zwlr_output_head_v1::{self, ZwlrOutputHeadV1},
    zwlr_output_manager_v1::{self, ZwlrOutputManagerV1},
    zwlr_output_mode_v1::{self, ZwlrOutputModeV1},
};

const DEADLINE: Duration = Duration::from_secs(2);

pub fn failed(error: impl std::fmt::Display) -> Error {
    Error::Failed(format!("Hyprland output management: {error}"))
}

fn socket_path(dir: &Path, lock: &str) -> Result<PathBuf, Error> {
    let mut lines = lock.lines();
    let pid = lines.next().and_then(|pid| pid.parse::<u32>().ok());
    let socket = lines.next().map(Path::new);
    let Some(socket) = socket.filter(|_| pid.is_some_and(|pid| pid > 0)) else {
        return Err(failed("invalid hyprland.lock"));
    };
    if lines.any(|line| !line.is_empty()) {
        return Err(failed("invalid hyprland.lock"));
    }
    if socket.is_absolute() {
        return Ok(socket.to_owned());
    }
    let mut components = socket.components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(failed("invalid Wayland socket in hyprland.lock"));
    }
    let runtime = dir
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| failed("invalid Hyprland instance directory"))?;
    Ok(runtime.join(socket))
}

pub async fn socket(dir: &Path) -> Result<UnixStream, Error> {
    let lock = tokio::fs::read_to_string(dir.join("hyprland.lock"))
        .await
        .map_err(failed)?;
    let path = socket_path(dir, &lock)?;
    tokio::time::timeout(DEADLINE, tokio::net::UnixStream::connect(path))
        .await
        .map_err(failed)?
        .map_err(failed)?
        .into_std()
        .map_err(failed)
}

pub struct Manager {
    connection: Connection,
    queue: EventQueue<State>,
    state: State,
    committed: HashMap<String, bool>,
}

#[derive(Default)]
struct State {
    manager: Option<ZwlrOutputManagerV1>,
    heads: Vec<Head>,
    serial: Option<u32>,
    sync: Option<wl_callback::WlCallback>,
    configuration: Option<ZwlrOutputConfigurationV1>,
    finished: bool,
    result: Option<Result<(), Error>>,
}

struct Head {
    proxy: ZwlrOutputHeadV1,
    name: String,
}

impl Manager {
    pub fn new(socket: UnixStream) -> Result<Self, Error> {
        let connection = Connection::from_socket(socket).map_err(failed)?;
        let queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let mut manager = Self {
            connection,
            queue,
            state: State::default(),
            committed: HashMap::new(),
        };
        manager.sync()?;
        if manager.state.manager.is_none() {
            return Err(Error::Unsupported(
                "Hyprland does not expose wlr-output-management".to_owned(),
            ));
        }
        manager.wait(|state| state.serial.is_some())?;
        Ok(manager)
    }

    pub fn set_enabled(
        &mut self,
        connector: &str,
        enabled: bool,
        disabled: &[String],
    ) -> Result<(), Error> {
        self.sync()?;
        let mut changes: HashMap<String, bool> =
            disabled.iter().map(|name| (name.clone(), false)).collect();
        changes.insert(connector.to_owned(), enabled);
        let configuration = self.configure(&changes)?;
        self.state.result = None;
        self.state.configuration = Some(configuration.clone());
        configuration.apply();
        let result = self.wait(|state| state.result.is_some());
        self.state.configuration = None;
        configuration.destroy();
        self.connection.flush().map_err(failed)?;
        result?;
        self.state
            .result
            .take()
            .ok_or_else(|| failed("output manager returned no result"))?
    }

    pub fn restore_enabled(&mut self, connector: &str, enabled: bool) -> Result<(), Error> {
        let configuration = self.configure(&HashMap::from([(connector.to_owned(), enabled)]))?;
        configuration.apply();
        configuration.destroy();
        self.sync()
    }

    pub fn confirm(&mut self, connector: String, enabled: bool, disabled: Vec<String>) {
        self.committed
            .extend(disabled.into_iter().map(|name| (name, false)));
        self.committed.insert(connector, enabled);
    }

    pub fn has_committed(&self) -> bool {
        !self.committed.is_empty()
    }

    pub fn replacement(socket: UnixStream, previous: &Self) -> Result<Self, Error> {
        let mut replacement = Self::new(socket)?;
        let configuration = replacement.configure(&previous.committed)?;
        configuration.apply();
        configuration.destroy();
        replacement.sync()?;
        replacement.committed = previous.committed.clone();
        Ok(replacement)
    }

    fn configure(
        &self,
        changes: &HashMap<String, bool>,
    ) -> Result<ZwlrOutputConfigurationV1, Error> {
        for connector in changes.keys() {
            if !self.state.heads.iter().any(|head| head.name == *connector) {
                return Err(Error::Refused(format!("unknown output {connector}")));
            }
        }
        let manager = self
            .state
            .manager
            .as_ref()
            .ok_or_else(|| failed("output manager disappeared"))?;
        let serial = self
            .state
            .serial
            .ok_or_else(|| failed("output manager has no serial"))?;
        let configuration = manager.create_configuration(serial, &self.queue.handle(), ());
        for head in &self.state.heads {
            if let Some(enabled) = changes.get(&head.name) {
                if *enabled {
                    configuration.enable_head(&head.proxy, &self.queue.handle(), ());
                } else {
                    configuration.disable_head(&head.proxy);
                }
            }
        }
        Ok(configuration)
    }

    fn sync(&mut self) -> Result<(), Error> {
        self.state.sync = Some(self.connection.display().sync(&self.queue.handle(), ()));
        self.wait(|state| state.sync.is_none())
    }

    fn wait(&mut self, done: impl Fn(&State) -> bool) -> Result<(), Error> {
        let deadline = Instant::now() + DEADLINE;
        loop {
            self.queue
                .dispatch_pending(&mut self.state)
                .map_err(failed)?;
            if self.state.finished {
                return Err(failed("output manager stopped"));
            }
            if done(&self.state) {
                return Ok(());
            }
            self.connection.flush().map_err(failed)?;
            let Some(guard) = self.queue.prepare_read() else {
                continue;
            };
            let left = deadline
                .checked_duration_since(Instant::now())
                .filter(|left| !left.is_zero())
                .ok_or_else(|| failed("the compositor did not answer in time"))?;
            let timeout = Timespec {
                tv_sec: left.as_secs() as i64,
                tv_nsec: i64::from(left.subsec_nanos()),
            };
            let fd = guard.connection_fd();
            let mut fds = [PollFd::new(&fd, PollFlags::IN)];
            match poll(&mut fds, Some(&timeout)) {
                Ok(0) => return Err(failed("the compositor did not answer in time")),
                Ok(_) => {
                    guard.read().map_err(failed)?;
                }
                Err(rustix::io::Errno::INTR) => {}
                Err(error) => return Err(failed(error)),
            }
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
            && interface == ZwlrOutputManagerV1::interface().name
        {
            state.manager = Some(registry.bind(name, version.min(4), handle, ()));
        }
    }
}

impl Dispatch<ZwlrOutputManagerV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ZwlrOutputManagerV1,
        event: zwlr_output_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_output_manager_v1::Event::Head { head } => state.heads.push(Head {
                proxy: head,
                name: String::new(),
            }),
            zwlr_output_manager_v1::Event::Done { serial } => state.serial = Some(serial),
            zwlr_output_manager_v1::Event::Finished => state.finished = true,
            _ => {}
        }
    }

    event_created_child!(State, ZwlrOutputManagerV1, [0 => (ZwlrOutputHeadV1, ())]);
}

impl Dispatch<ZwlrOutputHeadV1, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &ZwlrOutputHeadV1,
        event: zwlr_output_head_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_output_head_v1::Event::Finished = event {
            state.heads.retain(|head| head.proxy != *proxy);
            if proxy.version() >= 3 {
                proxy.release();
            }
            return;
        }
        let Some(head) = state.heads.iter_mut().find(|head| head.proxy == *proxy) else {
            return;
        };
        if let zwlr_output_head_v1::Event::Name { name } = event {
            head.name = name;
        }
    }

    event_created_child!(State, ZwlrOutputHeadV1, [3 => (ZwlrOutputModeV1, ())]);
}

impl Dispatch<ZwlrOutputConfigurationV1, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &ZwlrOutputConfigurationV1,
        event: zwlr_output_configuration_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if state.configuration.as_ref() != Some(proxy) {
            return;
        }
        state.result = match event {
            zwlr_output_configuration_v1::Event::Succeeded => Some(Ok(())),
            zwlr_output_configuration_v1::Event::Failed => {
                Some(Err(failed("configuration refused")))
            }
            zwlr_output_configuration_v1::Event::Cancelled => {
                Some(Err(failed("outputs changed during configuration")))
            }
            _ => None,
        };
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &wl_callback::WlCallback,
        _: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if state.sync.as_ref() == Some(proxy) {
            state.sync = None;
        }
    }
}

impl Dispatch<ZwlrOutputModeV1, ()> for State {
    fn event(
        _: &mut Self,
        proxy: &ZwlrOutputModeV1,
        event: zwlr_output_mode_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_output_mode_v1::Event::Finished = event
            && proxy.version() >= 3
        {
            proxy.release();
        }
    }
}

delegate_noop!(State: ignore ZwlrOutputConfigurationHeadV1);

#[cfg(test)]
mod tests;
use std::collections::HashMap;
