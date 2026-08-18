use std::rc::Rc;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::{
    utils::popover_scroll,
    widgets::{
        animated_popover::AnimatedPopover, empty_state::EmptyState, popover_shell::PopoverShell,
    },
};

use super::{
    protocol::{EventKind, EventPayload, EventSource, TreeNode},
    renderer::RenderCatalog,
};

const DEFAULT_SIZE_CLASS: &str = "popover-size-medium";

pub struct Popover {
    root_node: Option<TreeNode>,
    content_box: gtk::Box,
    popover: AnimatedPopover,
    size_class: &'static str,
    applet_css_class: Option<String>,
}

pub struct Init {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum Input {
    Toggle,
    Close,
    SetRoot(Option<TreeNode>),
    SetCssClass(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    Event(EventPayload),
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = Init;
    type Input = Input;
    type Output = Output;

    view! {
        root = AnimatedPopover {
            add_css_class: "popover-size-medium",

            PopoverShell {

                #[name = "scroller"]
                gtk::ScrolledWindow {
                    set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                    set_vexpand: false,
                    set_propagate_natural_height: true,

                    #[name = "content_box"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 0,
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        popover_scroll::install_half_monitor_limit(
            widgets.root.upcast_ref(),
            &widgets.scroller,
            &init.parent,
        );

        let opened_sender = sender.clone();
        widgets.root.connect_show(move |_| {
            let _ = opened_sender.output(Output::Event(popover_lifecycle_event(EventKind::Open)));
        });

        let closed_sender = sender.clone();
        widgets.root.connect_closed(move |_| {
            let _ = closed_sender.output(Output::Event(popover_lifecycle_event(EventKind::Close)));
        });

        let model = Popover {
            root_node: None,
            content_box: widgets.content_box.clone(),
            popover: widgets.root.clone(),
            size_class: DEFAULT_SIZE_CLASS,
            applet_css_class: None,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            Input::Toggle => self.popover.toggle(),
            Input::Close => self.popover.close(),
            Input::SetRoot(root) => {
                self.root_node = root;
                self.rebuild(&sender);
            }
            Input::SetCssClass(class) => {
                if let Some(previous) = &self.applet_css_class {
                    self.popover.remove_css_class(&format!("applet-{previous}"));
                }
                self.popover.add_css_class(&format!("applet-{class}"));
                self.applet_css_class = Some(class);
            }
        }
    }
}

fn popover_lifecycle_event(kind: EventKind) -> EventPayload {
    EventPayload {
        id: "popover".into(),
        kind,
        source: EventSource::Popover,
        button: None,
        active: None,
        value: None,
        delta_y: None,
    }
}

impl Popover {
    fn rebuild(&mut self, sender: &ComponentSender<Self>) {
        self.popover.remove_css_class(self.size_class);

        while let Some(child) = self.content_box.first_child() {
            self.content_box.remove(&child);
        }

        let Some(root) = &self.root_node else {
            self.size_class = DEFAULT_SIZE_CLASS;
            self.popover.add_css_class(self.size_class);
            return;
        };

        let output_sender = sender.output_sender().clone();
        let renderer = RenderCatalog::new(Rc::new(move |event| {
            output_sender.emit(Output::Event(event));
        }));

        if let TreeNode::PopoverShell(shell) = root {
            self.size_class = shell.size.class_name();
            self.popover.add_css_class(self.size_class);
            match renderer.render(root) {
                Ok(widget) => self.content_box.append(&widget),
                Err(error) => {
                    tracing::warn!(%error, "exec popover shell render failed");
                    self.content_box.append(&render_error_state());
                }
            }
        } else {
            self.size_class = DEFAULT_SIZE_CLASS;
            self.popover.add_css_class(self.size_class);
            match renderer.render(root) {
                Ok(widget) => self.content_box.append(&widget),
                Err(error) => {
                    tracing::warn!(%error, "exec popover render failed");
                    self.content_box.append(&render_error_state());
                }
            }
        }
    }
}

/// Shown in place of the tree when RenderCatalog::render fails, instead of
/// leaving the popover entirely blank with only a log line.
fn render_error_state() -> EmptyState {
    let empty = EmptyState::new();
    empty.set_title("Couldn't display this content");
    empty.set_subtitle(Some("Check the applet's logs for details."));
    empty
}
