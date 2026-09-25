use std::ops::RangeInclusive;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use wayland_client::protocol::{wl_callback, wl_registry};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_dispatch};

#[derive(Debug)]
pub enum Stall {
    TimedOut,
    Failed(String),
}

pub fn failed(error: impl std::fmt::Display) -> Stall {
    Stall::Failed(error.to_string())
}

impl From<Stall> for std::io::Error {
    fn from(stall: Stall) -> Self {
        match stall {
            Stall::TimedOut => Self::new(
                std::io::ErrorKind::TimedOut,
                "the compositor did not answer in time",
            ),
            Stall::Failed(reason) => Self::other(reason),
        }
    }
}

pub struct Global {
    pub interface: String,
    pub name: u32,
    pub version: u32,
}

pub struct Globals {
    registry: wl_registry::WlRegistry,
    list: Vec<Global>,
}

impl Globals {
    pub fn pairs(&self) -> Vec<(&str, u32)> {
        self.list
            .iter()
            .map(|global| (global.interface.as_str(), global.version))
            .collect()
    }

    pub fn all<'a>(&'a self, interface: &'a str) -> impl Iterator<Item = &'a Global> {
        self.list
            .iter()
            .filter(move |global| global.interface == interface)
    }

    pub fn registry(&self) -> &wl_registry::WlRegistry {
        &self.registry
    }

    pub fn bind<I, U, D>(
        &self,
        handle: &QueueHandle<D>,
        versions: RangeInclusive<u32>,
        udata: U,
    ) -> Option<I>
    where
        I: Proxy + 'static,
        U: Send + Sync + 'static,
        D: Dispatch<I, U> + 'static,
    {
        let global = self.all(I::interface().name).next()?;
        let version = global.version.min(*versions.end());
        (version >= *versions.start())
            .then(|| self.registry.bind(global.name, version, handle, udata))
    }
}

#[derive(Default)]
struct Listing {
    list: Vec<Global>,
}

pub fn connect(deadline: Instant) -> Result<(Connection, Globals), Stall> {
    let connection = Connection::connect_to_env().map_err(failed)?;
    let mut queue = connection.new_event_queue();
    let registry = connection.display().get_registry(&queue.handle(), ());
    let mut listing = Listing::default();
    roundtrip(&connection, &mut queue, &mut listing, deadline)?;
    Ok((
        connection,
        Globals {
            registry,
            list: listing.list,
        },
    ))
}

pub fn roundtrip<S>(
    connection: &Connection,
    queue: &mut EventQueue<S>,
    state: &mut S,
    deadline: Instant,
) -> Result<(), Stall>
where
    S: Dispatch<wl_callback::WlCallback, Synced> + 'static,
{
    let callback = connection
        .display()
        .sync(&queue.handle(), Synced(AtomicBool::new(false)));
    settle(connection, queue, state, deadline, |_| {
        callback
            .data::<Synced>()
            .is_some_and(|synced| synced.0.load(Ordering::Acquire))
    })
}

pub fn settle<S>(
    connection: &Connection,
    queue: &mut EventQueue<S>,
    state: &mut S,
    deadline: Instant,
    done: impl Fn(&S) -> bool,
) -> Result<(), Stall> {
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
            .ok_or(Stall::TimedOut)?;
        let timeout = Timespec {
            tv_sec: left.as_secs() as i64,
            tv_nsec: i64::from(left.subsec_nanos()),
        };
        let fd = guard.connection_fd();
        let mut fds = [PollFd::new(&fd, PollFlags::IN)];
        match poll(&mut fds, Some(&timeout)) {
            Ok(0) => return Err(Stall::TimedOut),
            Ok(_) => {
                guard.read().map_err(failed)?;
            }
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(failed(error)),
        }
    }
}

pub struct Synced(AtomicBool);

impl<D> Dispatch<wl_callback::WlCallback, Synced, D> for Synced
where
    D: Dispatch<wl_callback::WlCallback, Synced>,
{
    fn event(
        _: &mut D,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        synced: &Synced,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            synced.0.store(true, Ordering::Release);
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for Listing {
    fn event(
        state: &mut Self,
        _: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => state.list.push(Global {
                interface,
                name,
                version,
            }),
            wl_registry::Event::GlobalRemove { name } => {
                state.list.retain(|global| global.name != name);
            }
            _ => {}
        }
    }
}

delegate_dispatch!(Listing: [wl_callback::WlCallback: Synced] => Synced);
