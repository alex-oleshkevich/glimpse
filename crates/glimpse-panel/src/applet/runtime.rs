use std::any::Any;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use glimpse_ipc::{Client, Event};
use glimpse_widgets::IndicatorGroup;
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
    name: String,
    applet: Option<Box<dyn Applet>>,
    ctx: Ctx,
    group: IndicatorGroup,
    scroll: HashMap<String, (f64, f64)>,
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

        let ctx = Ctx::new(init.client, sender.command_sender().clone());
        let applet = (init.build)(&ctx);

        let mut model = AppletRuntime {
            name: init.name,
            applet: Some(applet),
            ctx,
            group: root,
            scroll: HashMap::new(),
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
                for direction in self.notches(&indicator, dx, dy) {
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
                    applet = self.name,
                    reason = panic_reason(panic.as_ref()),
                    "applet panicked, stopping it"
                );
                self.applet = None;
                Vec::new()
            }
        };

        self.group.set_items(&specs);
    }

    fn notches(&mut self, indicator: &str, dx: f64, dy: f64) -> Vec<Direction> {
        let accumulated = self.scroll.entry(indicator.to_owned()).or_default();
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
    pub name: String,
    pub group: IndicatorGroup,
    _controller: Controller<AppletRuntime>,
}

impl AppletHandle {
    pub fn launch(name: String, client: Client, build: Builder) -> Self {
        let controller = AppletRuntime::builder()
            .launch(AppletInit {
                name: name.clone(),
                client,
                build,
            })
            .detach();

        Self {
            name,
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
