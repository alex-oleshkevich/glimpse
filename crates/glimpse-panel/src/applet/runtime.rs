use std::any::Any;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use glimpse_ipc::{Client, Event};
use glimpse_widgets::{IndicatorGroup, IndicatorSpec};
use relm4::{Component, ComponentController, ComponentParts, ComponentSender, Controller};

use super::{Applet, Button, Ctx, Direction, Input, Pointer};

const NOTCH: f64 = 1.0;

pub type Builder = fn(&Ctx) -> Box<dyn Applet>;

pub struct AppletInit {
    pub name: String,
    pub client: Client,
    pub build: Builder,
}

#[derive(Debug)]
pub enum HostInput {
    Pressed { indicator: String, button: u32 },
    Scrolled { indicator: String, dx: f64, dy: f64 },
}

pub struct AppletRuntime {
    applet: Option<Box<dyn Applet>>,
    ctx: Ctx,
    group: IndicatorGroup,
    scroll: Scroll,
}

impl Component for AppletRuntime {
    type Init = AppletInit;
    type Input = HostInput;
    type Output = ();
    type CommandOutput = Event;
    type Root = IndicatorGroup;
    type Widgets = ();

    fn init_root() -> Self::Root {
        IndicatorGroup::new()
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.connect_pressed({
            let sender = sender.clone();
            move |_, indicator, button| {
                sender.input(HostInput::Pressed {
                    indicator: indicator.to_owned(),
                    button,
                });
            }
        });
        root.connect_scrolled({
            let sender = sender.clone();
            move |_, indicator, dx, dy| {
                sender.input(HostInput::Scrolled {
                    indicator: indicator.to_owned(),
                    dx,
                    dy,
                });
            }
        });

        let build = init.build;
        let ctx = Ctx::new(init.name, init.client, sender.command_sender().clone());
        let applet = build(&ctx);
        tracing::debug!(applet = ctx.name(), "started");

        let mut model = AppletRuntime {
            applet: Some(applet),
            ctx,
            group: root,
            scroll: Scroll::default(),
        };
        model.deliver(None);

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            HostInput::Pressed { indicator, button } => self.deliver(Some(&Input::Pointer {
                indicator,
                pointer: Pointer::Press(Button::from_code(button)),
            })),
            HostInput::Scrolled { indicator, dx, dy } => {
                for direction in self.scroll.notches(&indicator, dx, dy) {
                    self.deliver(Some(&Input::Pointer {
                        indicator: indicator.clone(),
                        pointer: Pointer::Scroll(direction),
                    }));
                }
            }
        }
    }

    fn update_cmd(&mut self, event: Event, _sender: ComponentSender<Self>, _root: &Self::Root) {
        self.deliver(Some(&Input::Topic(event)));
    }
}

impl AppletRuntime {
    fn deliver(&mut self, input: Option<&Input>) {
        match input {
            Some(Input::Topic(event)) => tracing::debug!(
                applet = self.ctx.name(),
                topic = event.topic,
                seq = event.seq,
                stale = event.stale,
                "event"
            ),
            Some(Input::Pointer { indicator, pointer }) => {
                tracing::debug!(applet = self.ctx.name(), indicator, ?pointer, "pointer")
            }
            None => {}
        }

        let outcome = {
            let Some(applet) = self.applet.as_mut() else {
                return;
            };
            let ctx = &self.ctx;
            catch_unwind(AssertUnwindSafe(|| {
                if let Some(input) = input {
                    applet.handle(ctx, input);
                }
                applet.indicators()
            }))
        };

        let specs = match outcome {
            Ok(specs) => specs,
            Err(panic) => {
                tracing::error!(
                    applet = self.ctx.name(),
                    reason = panic_reason(panic.as_ref()),
                    "applet panicked, stopping it"
                );
                self.applet = None;
                self.ctx.shutdown();
                Vec::new()
            }
        };

        tracing::debug!(
            applet = self.ctx.name(),
            indicators = specs.len(),
            "rendered"
        );
        self.scroll.prune(&specs);
        self.group.set_items(&specs);
    }
}

#[derive(Default)]
struct Scroll {
    accumulated: HashMap<String, (f64, f64)>,
}

impl Scroll {
    fn notches(&mut self, indicator: &str, dx: f64, dy: f64) -> Vec<Direction> {
        let accumulated = self.accumulated.entry(indicator.to_owned()).or_default();
        let mut out = Vec::new();
        drain(
            &mut accumulated.0,
            dx,
            Direction::Left,
            Direction::Right,
            &mut out,
        );
        drain(
            &mut accumulated.1,
            dy,
            Direction::Up,
            Direction::Down,
            &mut out,
        );
        out
    }

    fn prune(&mut self, specs: &[IndicatorSpec]) {
        self.accumulated
            .retain(|indicator, _| specs.iter().any(|spec| &spec.id == indicator));
    }
}

fn drain(
    accumulated: &mut f64,
    delta: f64,
    negative: Direction,
    positive: Direction,
    out: &mut Vec<Direction>,
) {
    *accumulated += delta;
    while *accumulated >= NOTCH {
        *accumulated -= NOTCH;
        out.push(positive);
    }
    while *accumulated <= -NOTCH {
        *accumulated += NOTCH;
        out.push(negative);
    }
}

fn panic_reason(panic: &(dyn Any + Send)) -> &str {
    if let Some(text) = panic.downcast_ref::<&str>() {
        return text;
    }
    if let Some(text) = panic.downcast_ref::<String>() {
        return text;
    }
    "panicked"
}

pub struct AppletHandle {
    pub group: IndicatorGroup,
    _controller: Controller<AppletRuntime>,
}

impl AppletHandle {
    pub fn launch(name: String, client: Client, build: Builder) -> Self {
        let controller = AppletRuntime::builder()
            .launch(AppletInit {
                name,
                client,
                build,
            })
            .detach();

        Self {
            group: controller.widget().clone(),
            _controller: controller,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drained(deltas: &[f64]) -> Vec<Direction> {
        let mut accumulated = 0.0;
        let mut out = Vec::new();
        for delta in deltas {
            drain(
                &mut accumulated,
                *delta,
                Direction::Up,
                Direction::Down,
                &mut out,
            );
        }
        out
    }

    fn spec(id: &str) -> IndicatorSpec {
        IndicatorSpec {
            id: id.to_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn each_indicator_accumulates_on_its_own() {
        let mut scroll = Scroll::default();

        assert!(scroll.notches("a", 0.0, 0.6).is_empty());
        assert!(
            scroll.notches("b", 0.0, 0.6).is_empty(),
            "a second indicator must not inherit the first one's partial gesture"
        );
        assert_eq!(scroll.notches("a", 0.0, 0.6), [Direction::Down]);
        assert!(scroll.notches("b", 0.0, 0.3).is_empty());
    }

    #[test]
    fn an_indicator_that_stops_being_rendered_stops_being_tracked() {
        let mut scroll = Scroll::default();
        scroll.notches("a", 0.0, 0.6);
        scroll.notches("b", 0.0, 0.6);

        scroll.prune(&[spec("a")]);

        assert_eq!(scroll.accumulated.len(), 1);
        assert_eq!(
            scroll.notches("b", 0.0, 0.6),
            [],
            "a re-added indicator starts from zero, not from what it left behind"
        );
    }

    #[test]
    fn a_wheel_detent_is_one_notch() {
        assert_eq!(drained(&[1.0]), [Direction::Down]);
        assert_eq!(drained(&[-1.0]), [Direction::Up]);
    }

    #[test]
    fn a_touchpad_coalesces_into_whole_notches() {
        assert!(
            drained(&[0.4, 0.4]).is_empty(),
            "a partial gesture moves nothing"
        );
        assert_eq!(drained(&[0.4, 0.4, 0.4]), [Direction::Down]);
        assert_eq!(
            drained(&[0.4; 10]).len(),
            4,
            "four whole notches out of 4.0, not ten events"
        );
    }

    #[test]
    fn a_reversal_cancels_rather_than_firing_both_ways() {
        assert!(drained(&[0.6, -0.6]).is_empty());
        assert_eq!(drained(&[0.6, -1.6]), [Direction::Up]);
    }

    #[test]
    fn one_burst_larger_than_a_notch_fires_every_notch_it_contains() {
        assert_eq!(
            drained(&[2.5]),
            [Direction::Down, Direction::Down],
            "the remainder is carried, not discarded"
        );
        assert_eq!(drained(&[2.5, 0.5]), [Direction::Down; 3]);
    }
}
