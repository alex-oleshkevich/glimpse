use std::fmt::Debug;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::components::menu_item::{MenuItem, build_menu_popover};

/// A flat hamburger-icon button that opens a popover with `MenuItem<Cmd>`
/// entries on left-click. Clicking an entry emits its `command` on the
/// component's output channel.
///
/// The popover widget tree is the same one used by right-click context
/// menus (see `menu_item::attach_context_menu`); only the trigger differs.
pub struct MenuButton<Cmd: Clone + Debug + Send + 'static> {
    items: Vec<MenuItem<Cmd>>,
    button: gtk::MenuButton,
}

#[derive(Debug)]
pub enum MenuButtonInput<Cmd> {
    /// Replace the menu entries. The popover is rebuilt; if it was open,
    /// it closes.
    SetItems(Vec<MenuItem<Cmd>>),
}

#[relm4::component(pub)]
impl<Cmd> SimpleComponent for MenuButton<Cmd>
where
    Cmd: Clone + Debug + Send + 'static,
{
    type Init = Vec<MenuItem<Cmd>>;
    type Input = MenuButtonInput<Cmd>;
    type Output = Cmd;

    view! {
        #[root]
        gtk::MenuButton {
            set_icon_name: "open-menu-symbolic",
            set_has_frame: false,
            add_css_class: "flat",
            add_css_class: "menu-button",
        }
    }

    fn init(
        items: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let popover = build_menu_popover(&items, sender.output_sender().clone());
        root.set_popover(Some(&popover));
        root.set_sensitive(!items.is_empty());

        let model = MenuButton {
            items,
            button: root.clone(),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            MenuButtonInput::SetItems(items) => {
                let popover = build_menu_popover(&items, sender.output_sender().clone());
                self.button.set_popover(Some(&popover));
                self.button.set_sensitive(!items.is_empty());
                self.items = items;
            }
        }
    }
}
