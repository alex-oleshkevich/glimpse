use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk::prelude::*};

use crate::applets::exec::protocol::{
    EventKind, EventPayload, EventSource, MouseButton, StatusItem as StatusItemModel,
};
use crate::widgets::panel_indicator::PanelIndicator;

pub struct StatusItem {
    item: StatusItemModel,
    has_popover: bool,
}

#[derive(Debug, Clone)]
pub struct Init {
    pub item: StatusItemModel,
    pub has_popover: bool,
}

#[derive(Debug)]
pub enum Input {
    Click(u32),
    Scroll(f64),
    Reconfigure {
        item: StatusItemModel,
        has_popover: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    TogglePopover,
    ContextMenu,
    Event(EventPayload),
    Activate(Option<EventPayload>),
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for StatusItem {
    type Init = Init;
    type Input = Input;
    type Output = Output;

    view! {
        PanelIndicator {
            add_css_class: "exec-status-item",
            #[watch]
            set_tooltip_text: model.item.tooltip.as_deref(),
            #[watch]
            set_icon: model.item.icon.as_deref(),
            #[watch]
            set_label: model.item.label.as_deref(),
            connect_activated[sender] => move |_| {
                sender.input(Input::Click(1));
            },
            connect_middle_clicked[sender] => move |_| {
                sender.input(Input::Click(2));
            },
            connect_secondary_clicked[sender] => move |_| {
                sender.input(Input::Click(3));
            },
            connect_scrolled[sender] => move |_, _dx, dy| {
                sender.input(Input::Scroll(dy));
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = StatusItem {
            item: init.item,
            has_popover: init.has_popover,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            Input::Click(button) => {
                if button == 3 {
                    let _ = sender.output(Output::ContextMenu);
                    return;
                }

                let event = self.item.id.as_ref().map(|id| EventPayload {
                    id: id.clone(),
                    kind: EventKind::Click,
                    source: EventSource::Status,
                    button: Some(MouseButton::from_number(button)),
                    active: None,
                    value: None,
                    delta_y: None,
                });
                if button == 1 {
                    let output = match event {
                        Some(event) => Output::Activate(Some(event)),
                        None if self.has_popover => Output::TogglePopover,
                        None => return,
                    };
                    let _ = sender.output(output);
                    return;
                }

                if let Some(event) = event {
                    let _ = sender.output(Output::Event(event));
                }
            }
            Input::Scroll(delta_y) => {
                if let Some(id) = &self.item.id {
                    let _ = sender.output(Output::Event(EventPayload {
                        id: id.clone(),
                        kind: EventKind::Scroll,
                        source: EventSource::Status,
                        button: None,
                        active: None,
                        value: None,
                        delta_y: Some(delta_y),
                    }));
                }
            }
            Input::Reconfigure { item, has_popover } => {
                self.item = item;
                self.has_popover = has_popover;
            }
        }
    }
}
