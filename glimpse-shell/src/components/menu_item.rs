#![allow(dead_code)]

use std::rc::Rc;

use relm4::gtk::{self, gdk, glib, prelude::*};

/// Data describing one entry in a context (right-click) menu.
///
/// `command` is emitted on the channel the consumer provides when the
/// entry is activated — see [`attach_context_menu`].
#[derive(Debug, Clone)]
pub struct MenuItem<Cmd> {
    pub label: String,
    pub icon: Option<String>,
    pub command: Cmd,
    pub enabled: bool,
    pub destructive: bool,
    pub visible: bool,
}

impl<Cmd> MenuItem<Cmd> {
    pub fn new(label: impl Into<String>, command: Cmd) -> Self {
        Self {
            label: label.into(),
            icon: None,
            command,
            enabled: true,
            destructive: false,
            visible: true,
        }
    }

    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    pub fn hidden(mut self) -> Self {
        self.visible = false;
        self
    }
}

/// Attach a right-click context menu populated from `items` to `widget`.
///
/// When an entry is activated, its `command` is sent on `sender`. The
/// popover is parented to the widget and closes itself after activation.
pub fn attach_context_menu<W, Cmd>(
    widget: &W,
    items: Vec<MenuItem<Cmd>>,
    sender: relm4::Sender<Cmd>,
) where
    W: IsA<gtk::Widget>,
    Cmd: Clone + 'static,
{
    if items.iter().all(|i| !i.visible) {
        return;
    }

    let popover = build_menu_popover(&items, sender);
    popover.set_parent(widget);

    let click = gtk::GestureClick::new();
    click.set_button(gdk::BUTTON_SECONDARY);
    let pop_ref = popover.clone();
    click.connect_pressed(move |_, _, x, y| {
        let rect = gdk::Rectangle::new(x as i32, y as i32, 1, 1);
        pop_ref.set_pointing_to(Some(&rect));
        pop_ref.popup();
    });
    widget.add_controller(click);
}

/// Build a `gtk::Popover` whose body is a flat list of action buttons,
/// one per entry in `items`. Each entry's `command` is sent on `sender`
/// when activated; the popover closes itself afterward.
///
/// Used by both [`attach_context_menu`] (right-click trigger) and
/// `MenuButton` (left-click trigger on a `gtk::MenuButton`).
pub(crate) fn build_menu_popover<Cmd>(
    items: &[MenuItem<Cmd>],
    sender: relm4::Sender<Cmd>,
) -> gtk::Popover
where
    Cmd: Clone + 'static,
{
    let popover = gtk::Popover::new();
    popover.set_has_arrow(false);
    popover.set_autohide(true);
    popover.add_css_class("context-menu");
    popover.add_css_class("popover-size-small");

    let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    menu_box.add_css_class("action-menu");

    let sender = Rc::new(sender);

    for item in items.iter().filter(|i| i.visible) {
        let button = gtk::Button::new();
        button.add_css_class("flat");
        button.add_css_class("menu-item");
        button.set_sensitive(item.enabled);
        if item.destructive {
            button.add_css_class("destructive-action");
        }

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        if let Some(icon_name) = &item.icon {
            let img = gtk::Image::from_icon_name(icon_name);
            img.set_pixel_size(16);
            row.append(&img);
        }
        let label = gtk::Label::new(Some(&item.label));
        label.set_xalign(0.0);
        label.set_hexpand(true);
        row.append(&label);
        button.set_child(Some(&row));

        let cmd = item.command.clone();
        let sender = sender.clone();
        let popover_ref = popover.clone();
        button.connect_clicked(move |_| {
            let _ = sender.send(cmd.clone());
            popover_ref.popdown();
        });

        menu_box.append(&button);
    }

    popover.set_child(Some(&menu_box));
    let _ = glib::MainContext::default();
    popover
}
