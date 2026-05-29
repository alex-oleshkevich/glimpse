use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk::prelude::*};

use crate::applets::exec::protocol::{
    EventKind, EventPayload, EventSource, MouseButton, StatusItem as StatusItemModel,
};
use crate::widgets::panel_indicator::{PanelIndicator, PanelMenu, PanelMenuItem};

pub struct StatusItem {
    item: StatusItemModel,
    has_popover: bool,
}

#[derive(Debug, Clone)]
pub struct Init {
    pub item: StatusItemModel,
    pub has_popover: bool,
}

#[derive(Debug, Clone)]
pub enum Input {
    Click(u32),
    Scroll(f64),
    RestartCommand,
    Reconfigure {
        item: StatusItemModel,
        has_popover: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    TogglePopover,
    ContextMenu,
    RestartCommand,
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
            set_extra_classes: &applet_css_classes(&model.item),
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
        root.set_context_menu(
            PanelMenu {
                items: vec![PanelMenuItem::Action {
                    label: "Restart".into(),
                    input: Input::RestartCommand,
                    enabled: true,
                }],
            },
            sender.input_sender().clone(),
        );
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            Input::Click(button) => {
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
            Input::RestartCommand => {
                let _ = sender.output(Output::RestartCommand);
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

/// Filters the applet-supplied class list before it reaches
/// `PanelIndicator::set_extra_classes`. Two cases are stripped:
///
/// * Empty strings, which GTK rejects with a runtime warning.
/// * The literal `"exec-status-item"` base class. If an applet emitted it,
///   it would be added to the indicator's "extras" tracking set, and the
///   next update with that class absent would remove it from the widget —
///   wiping the base class the shell relies on to style the indicator.
fn applet_css_classes(item: &StatusItemModel) -> Vec<&str> {
    item.css_classes
        .iter()
        .map(String::as_str)
        .filter(|class| !class.is_empty() && *class != "exec-status-item")
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{StatusItemModel, applet_css_classes};

    #[test]
    fn applet_css_classes_drops_base_class_and_empty_strings() {
        let item = StatusItemModel {
            css_classes: vec![
                "threshold-warn".into(),
                "".into(),
                "exec-status-item".into(),
                "sysmonitor-cpu".into(),
            ],
            ..StatusItemModel::default()
        };
        assert_eq!(
            applet_css_classes(&item),
            vec!["threshold-warn", "sysmonitor-cpu"]
        );
    }

    #[test]
    fn applet_css_classes_preserves_duplicates_for_widget_layer_dedup() {
        // Duplicates pass through here; `set_extra_classes` dedupes when it
        // applies them. That keeps the filter pure and pushes order/dedup
        // policy to one place rather than splitting it across two layers.
        let item = StatusItemModel {
            css_classes: vec!["warn".into(), "warn".into(), "crit".into()],
            ..StatusItemModel::default()
        };
        assert_eq!(applet_css_classes(&item), vec!["warn", "warn", "crit"]);
    }
}
