use std::cell::{OnceCell, RefCell};

use gtk4::gdk;
use gtk4::prelude::*;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::{
    self, Capability, ExtBackgroundEffectManagerV1,
};

const BLURRED: &str = "blurred";

pub fn apply(window: &impl IsA<gtk4::Window>, enabled: bool) {
    let gtk_ok = gtk4::check_version(4, 23, 3).is_none();
    apply_with(
        window,
        active(enabled, gtk_ok, || Probe::get().is_some_and(Probe::blurs)),
    );
}

pub(crate) fn apply_with(window: &impl IsA<gtk4::Window>, active: bool) {
    let window = window.upcast_ref::<gtk4::Window>();
    if active {
        window.add_css_class(BLURRED);
    } else {
        window.remove_css_class(BLURRED);
    }
}

fn active(enabled: bool, gtk_ok: bool, compositor_blur: impl FnOnce() -> bool) -> bool {
    let compositor = (enabled && gtk_ok).then(compositor_blur);
    let active = compositor == Some(true);
    tracing::debug!(enabled, gtk_ok, compositor_blur = ?compositor, active, "blur");
    active
}

#[derive(Default)]
struct Events {
    blur: bool,
}

impl Dispatch<WlRegistry, GlobalListContents> for Events {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtBackgroundEffectManagerV1, ()> for Events {
    fn event(
        events: &mut Self,
        _: &ExtBackgroundEffectManagerV1,
        event: ext_background_effect_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_background_effect_manager_v1::Event::Capabilities { flags } = event {
            events.blur = matches!(flags, WEnum::Value(flags) if flags.contains(Capability::Blur));
        }
    }
}

struct Probe {
    queue: RefCell<EventQueue<Events>>,
    events: RefCell<Events>,
    _manager: ExtBackgroundEffectManagerV1,
}

impl Probe {
    fn bind() -> Option<&'static Self> {
        let display = gdk::Display::default()?;
        let connection = crate::connection::of(&display)
            .inspect_err(
                |error| tracing::debug!(%error, "no Wayland connection; blur is unavailable"),
            )
            .ok()?;
        let (globals, mut queue) = registry_queue_init::<Events>(&connection)
            .inspect_err(
                |error| tracing::debug!(%error, "cannot list globals; blur is unavailable"),
            )
            .ok()?;
        let manager = globals
            .bind::<ExtBackgroundEffectManagerV1, _, _>(&queue.handle(), 1..=1, ())
            .inspect_err(
                |error| tracing::debug!(%error, "no background effect; blur is unavailable"),
            )
            .ok()?;
        let mut events = Events::default();
        queue.roundtrip(&mut events).ok()?;
        tracing::debug!(blur = events.blur, "background effect bound");
        Some(Box::leak(Box::new(Self {
            queue: RefCell::new(queue),
            events: RefCell::new(events),
            _manager: manager,
        })))
    }

    fn get() -> Option<&'static Self> {
        thread_local! {
            static PROBE: OnceCell<Option<&'static Probe>> = const { OnceCell::new() };
        }
        PROBE
            .try_with(|cell| *cell.get_or_init(Self::bind))
            .ok()
            .flatten()
    }

    fn blurs(&self) -> bool {
        let mut events = self.events.borrow_mut();
        if let Err(error) = self.queue.borrow_mut().dispatch_pending(&mut events) {
            tracing::debug!(%error, "background effect events");
        }
        events.blur
    }
}

#[cfg(test)]
mod tests;
