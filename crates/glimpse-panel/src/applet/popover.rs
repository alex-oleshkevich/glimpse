use glimpse_ipc::{Client, Event};
use gtk4::prelude::*;
use relm4::{Component, ComponentController, Controller};

use super::Ctx;

#[derive(Clone)]
pub struct Seat {
    name: String,
    output: Option<String>,
    client: Client,
}

impl Seat {
    pub(crate) fn new(name: String, output: Option<String>, client: Client) -> Self {
        Self {
            name,
            output,
            client,
        }
    }

    pub fn ctx(&self, events: relm4::Sender<Event>) -> Ctx {
        Ctx::new(
            format!("{}.popover", self.name),
            self.output.clone(),
            self.client.clone(),
            events,
        )
    }
}

pub trait PopoverHandle {
    fn root(&self) -> gtk4::Widget;
}

pub struct Live<C: Component>(Controller<C>);

impl<C> Live<C>
where
    C: Component,
    C::Root: IsA<gtk4::Widget>,
{
    pub fn boxed(controller: Controller<C>) -> Box<dyn PopoverHandle> {
        Box::new(Self(controller))
    }
}

impl<C> PopoverHandle for Live<C>
where
    C: Component,
    C::Root: IsA<gtk4::Widget>,
{
    fn root(&self) -> gtk4::Widget {
        self.0.widget().clone().upcast()
    }
}
