use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

use adw::prelude::*;
use glimpse_config::Transition;
use gtk4::{cairo, gdk, gio, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use relm4::{Component, ComponentParts, ComponentSender, gtk};

use glimpse_widgets::raster::{self, Raster};

use crate::resolve::{Intent, RenderKey, Role};

pub struct Config {
    pub monitor: gdk::Monitor,
    pub role: Role,
    pub intent: Intent,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Config")
            .field("monitor", &self.monitor.connector())
            .field("role", &self.role)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum Input {
    Configure(Config),
    Reconfigure,
    TransitionDone(adw::TimedAnimation),
}

#[derive(Debug)]
pub struct Decoded {
    key: RenderKey,
    raster: anyhow::Result<Raster>,
}

pub struct Surface {
    window: gtk::Window,
    base: gtk::Picture,
    under: gtk::Picture,
    over: gtk::Picture,
    monitor: gdk::Monitor,
    role: Role,
    intent: Intent,
    wanted: Option<RenderKey>,
    rendered: Option<RenderKey>,
    decoding: Option<RenderKey>,
    animation: Option<adw::TimedAnimation>,
    seen: bool,
    notify: Vec<glib::SignalHandlerId>,
    image_monitor: Option<gio::FileMonitor>,
    image_changed: Rc<Cell<bool>>,
}

#[relm4::component(pub)]
impl Component for Surface {
    type Init = Config;
    type Input = Input;
    type Output = ();
    type CommandOutput = Decoded;

    view! {
        root = gtk::Window {
            #[name = "overlay"]
            gtk::Overlay {
                #[name = "base"]
                #[wrap(Some)]
                set_child = &gtk::Picture {
                    set_content_fit: gtk::ContentFit::Fill,
                },

                #[name = "under"]
                add_overlay = &gtk::Picture {},

                #[name = "over"]
                add_overlay = &gtk::Picture {},
            }
        }
    }

    fn init(
        config: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.init_layer_shell();
        root.set_layer(Layer::Background);
        root.set_namespace(Some(config.role.namespace()));
        root.set_monitor(Some(&config.monitor));
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            root.set_anchor(edge, true);
        }
        root.set_exclusive_zone(-1);
        root.set_keyboard_mode(KeyboardMode::None);

        let window = root.clone();
        let empty_region_window = window.clone();
        window.connect_realize(move |_| {
            if let Some(surface) = empty_region_window.surface() {
                surface.set_input_region(Some(&cairo::Region::create()));
            }
        });

        let widgets = view_output!();

        let notify = connect_monitor(&config.monitor, &sender);
        let image_changed = Rc::new(Cell::new(false));
        let image_monitor = watch_image(config.intent.image.as_deref(), &image_changed, &sender);
        let mut model = Surface {
            window: window.clone(),
            base: widgets.base.clone(),
            under: widgets.under.clone(),
            over: widgets.over.clone(),
            monitor: config.monitor,
            role: config.role,
            intent: config.intent,
            wanted: None,
            rendered: None,
            decoding: None,
            animation: None,
            seen: false,
            notify,
            image_monitor,
            image_changed,
        };
        model
            .base
            .set_paintable(Some(&raster::solid(model.intent.color)));
        model.settle(&sender);

        window.present();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            Input::Configure(config) => self.configure(config, &sender),
            Input::Reconfigure => {
                if self.image_changed.replace(false) {
                    self.rendered = None;
                }
                self.settle(&sender);
            }
            Input::TransitionDone(done) => finish(&self.over, &mut self.animation, &done),
        }
    }

    fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        self.decoded(message.key, message.raster, &sender);
    }

    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        for handler in self.notify.drain(..) {
            self.monitor.disconnect(handler);
        }
        if let Some(monitor) = self.image_monitor.take() {
            monitor.cancel();
        }
        self.decoding = None;
    }
}

impl Surface {
    fn configure(&mut self, config: Config, sender: &ComponentSender<Self>) {
        if self.monitor != config.monitor {
            for handler in self.notify.drain(..) {
                self.monitor.disconnect(handler);
            }
            self.notify = connect_monitor(&config.monitor, sender);
            self.window.set_monitor(Some(&config.monitor));
            self.monitor = config.monitor;
        }

        if self.intent.image != config.intent.image {
            if let Some(monitor) = self.image_monitor.take() {
                monitor.cancel();
            }
            self.image_monitor =
                watch_image(config.intent.image.as_deref(), &self.image_changed, sender);
        }

        self.base
            .set_paintable(Some(&raster::solid(config.intent.color)));

        if config.intent.image.is_none() && self.rendered.is_some() {
            self.clear_image();
        }

        self.intent = config.intent;
        self.settle(sender);
    }

    fn clear_image(&mut self) {
        if let Some(animation) = self.animation.take() {
            animation.skip();
        }
        self.under.set_paintable(gdk::Paintable::NONE);
        self.over.set_paintable(gdk::Paintable::NONE);
        self.over.set_opacity(0.0);
        self.rendered = None;
    }

    fn settle(&mut self, sender: &ComponentSender<Self>) {
        self.wanted = self.compute_wanted();

        let Some(wanted) = self.wanted.clone() else {
            return;
        };
        if self.rendered.as_ref() == Some(&wanted) {
            return;
        }
        if self.decoding.is_some() {
            return;
        }
        self.spawn_decode(wanted, sender);
    }

    fn output_target(&self) -> Option<raster::Target> {
        let geometry = self.monitor.geometry();
        if geometry.width() <= 0 || geometry.height() <= 0 {
            return None;
        }
        let scale = match self.monitor.scale() {
            scale if scale > 0.0 => scale,
            _ => f64::from(self.monitor.scale_factor()),
        };
        Some(raster::output_target(
            geometry.width(),
            geometry.height(),
            scale,
        ))
    }

    fn compute_wanted(&self) -> Option<RenderKey> {
        let output_target = self.output_target()?;
        let image = self.intent.image.clone()?;
        let target = match self.role {
            Role::Wallpaper => output_target,
            Role::Backdrop => raster::backdrop_target(output_target, self.intent.downscale_factor),
        };

        let mtime = self
            .rendered
            .as_ref()
            .filter(|rendered| rendered.image == image)
            .and_then(|rendered| rendered.mtime);

        Some(RenderKey {
            image,
            target,
            fit: self.intent.fit,
            blur_radius: self.intent.blur_radius,
            mtime,
        })
    }

    fn spawn_decode(&mut self, key: RenderKey, sender: &ComponentSender<Self>) {
        self.decoding = Some(key.clone());

        let path = key.image.clone();
        let target = key.target;
        let fit = key.fit;

        sender.spawn_oneshot_command(move || Decoded {
            key,
            raster: raster::raster(&path, target, fit),
        });
    }

    fn decoded(
        &mut self,
        key: RenderKey,
        raster: anyhow::Result<Raster>,
        sender: &ComponentSender<Self>,
    ) {
        self.decoding = None;

        if self.wanted.as_ref() == Some(&key) {
            match raster {
                Ok(raster) => {
                    self.rendered = Some(RenderKey {
                        mtime: Some(raster.mtime),
                        ..key
                    });
                    self.apply(raster, sender);
                }
                Err(err) => {
                    tracing::warn!(path = %key.image.display(), "wallpaper image failed to decode: {err:#}");
                    self.rendered = Some(key);
                }
            }
        }

        self.settle(sender);
    }

    fn apply(&mut self, raster: Raster, sender: &ComponentSender<Self>) {
        if let Some(animation) = self.animation.take() {
            animation.skip();
        }

        self.over.set_paintable(self.under.paintable().as_ref());
        self.over.set_content_fit(self.under.content_fit());
        self.over.set_opacity(1.0);

        let texture = raster::texture(&raster);
        let sigma = self.output_target().and_then(|output| {
            raster::blur_sigma(self.intent.blur_radius, raster.width, output.width)
        });
        let texture = sigma
            .zip(self.window.renderer())
            .and_then(|(sigma, renderer)| raster::blurred(&renderer, &texture, sigma))
            .unwrap_or(texture);
        self.under.set_paintable(Some(&texture));
        self.under
            .set_content_fit(raster::content_fit(self.intent.fit));

        if !self.seen
            || self.intent.transition == Transition::None
            || self.intent.transition_ms == 0
        {
            self.over.set_paintable(gdk::Paintable::NONE);
            self.over.set_opacity(0.0);
            self.seen = true;
            return;
        }

        let animation = adw::TimedAnimation::new(
            &self.over,
            1.0,
            0.0,
            self.intent.transition_ms,
            adw::PropertyAnimationTarget::new(&self.over, "opacity"),
        );
        let input_sender = sender.input_sender().clone();
        animation.connect_done(move |animation| {
            let _ = input_sender.send(Input::TransitionDone(animation.clone()));
        });
        animation.play();
        self.animation = Some(animation);
    }
}

fn finish(
    over: &gtk::Picture,
    animation: &mut Option<adw::TimedAnimation>,
    done: &adw::TimedAnimation,
) {
    if animation.as_ref() == Some(done) {
        over.set_paintable(gdk::Paintable::NONE);
        *animation = None;
    }
}

fn connect_monitor(
    monitor: &gdk::Monitor,
    sender: &ComponentSender<Surface>,
) -> Vec<glib::SignalHandlerId> {
    let reconfigure = || {
        let input_sender = sender.input_sender().clone();
        move |_: &gdk::Monitor| {
            let _ = input_sender.send(Input::Reconfigure);
        }
    };

    vec![
        monitor.connect_geometry_notify(reconfigure()),
        monitor.connect_scale_notify(reconfigure()),
        monitor.connect_scale_factor_notify(reconfigure()),
    ]
}

fn watch_image(
    image: Option<&Path>,
    changed: &Rc<Cell<bool>>,
    sender: &ComponentSender<Surface>,
) -> Option<gio::FileMonitor> {
    let image = image?;
    let parent = image.parent()?;
    let monitor = gio::File::for_path(parent)
        .monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
        .ok()?;

    let target = image.to_path_buf();
    let changed = changed.clone();
    let input_sender = sender.input_sender().clone();
    monitor.connect_changed(move |_, file, other, event| {
        if !should_reconfigure(event) || !touched(&target, file, other) {
            return;
        }
        changed.set(true);
        let _ = input_sender.send(Input::Reconfigure);
    });

    Some(monitor)
}

fn should_reconfigure(event: gio::FileMonitorEvent) -> bool {
    matches!(
        event,
        gio::FileMonitorEvent::Renamed
            | gio::FileMonitorEvent::Created
            | gio::FileMonitorEvent::MovedIn
            | gio::FileMonitorEvent::ChangesDoneHint
    )
}

fn touched(target: &Path, file: &gio::File, other: Option<&gio::File>) -> bool {
    [Some(file), other]
        .into_iter()
        .flatten()
        .any(|file| file.path().is_some_and(|path| path == target))
}

#[cfg(test)]
mod tests;
