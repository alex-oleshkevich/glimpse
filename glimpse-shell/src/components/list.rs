#![allow(dead_code)]

use std::fmt::Debug;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent, WidgetTemplate,
    factory::{DynamicIndex, FactoryComponent, FactorySender, FactoryVecDeque},
    gtk::{self, prelude::*},
};

use crate::components::menu_item::{MenuItem, attach_context_menu, build_menu_popover};

#[relm4::widget_template(pub)]
impl WidgetTemplate for ListItem {
    view! {
        #[name = "container"]
        gtk::Box {
            add_css_class: "flat",
            add_css_class: "list-item",
            add_css_class: "list-item__button",
            set_hexpand: false,

            gtk::Box {
                add_css_class: "list-item__content",
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                set_valign: gtk::Align::Center,

                #[name = "left"]
                gtk::Box {
                    add_css_class: "list-item__left",
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 0,
                    set_halign: gtk::Align::Start,
                    set_valign: gtk::Align::Center,
                    set_hexpand: false,
                    set_visible: false,
                },

                gtk::Box {
                    add_css_class: "list-item__text",
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 0,
                    set_hexpand: true,
                    // Center the text Box vertically so a single label
                    // aligns with the icon (instead of sitting at the
                    // top of the fill-height area). When sublabel is
                    // visible, both lines stay grouped and centered.
                    set_valign: gtk::Align::Center,

                    #[name = "label"]
                    gtk::Label {
                        add_css_class: "list-item__label",
                        set_halign: gtk::Align::Start,
                        set_xalign: 0.0,
                    },
                    #[name = "secondary_label"]
                    gtk::Label {
                        add_css_class: "list-item__secondary_label",
                        set_halign: gtk::Align::Start,
                        set_xalign: 0.0,
                    },
                },

                #[name = "right"]
                gtk::Box {
                    add_css_class: "list-item__right",
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 0,
                    set_halign: gtk::Align::End,
                    set_valign: gtk::Align::Center,
                    set_hexpand: false,
                    set_visible: false,
                },
            },
        }
    }
}

// ───────────────────────── Row model ──────────────────────────

/// Data for a single row in [`ListView`].
///
/// `menu_items` populates the row's **right-click** context menu.
/// `trailing_menu` populates a hamburger `gtk::MenuButton` rendered in
/// the row's right slot — clickable on **left-click**. Use either,
/// neither, or both.
///
/// `on_click_command` fires when the user clicks the row body (anywhere
/// not consumed by the trailing MenuButton).
#[derive(Debug, Clone)]
pub struct ListItemModel<Cmd> {
    pub label: String,
    pub sublabel: Option<String>,
    pub icon: Option<String>,
    pub tooltip: Option<String>,
    pub menu_items: Vec<MenuItem<Cmd>>,
    pub trailing_menu: Vec<MenuItem<Cmd>>,
    pub on_click_command: Option<Cmd>,
}

impl<Cmd> ListItemModel<Cmd> {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            sublabel: None,
            icon: None,
            tooltip: None,
            menu_items: Vec::new(),
            trailing_menu: Vec::new(),
            on_click_command: None,
        }
    }

    pub fn with_sublabel(mut self, sublabel: impl Into<String>) -> Self {
        self.sublabel = Some(sublabel.into());
        self
    }

    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn with_tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn with_menu(mut self, items: Vec<MenuItem<Cmd>>) -> Self {
        self.menu_items = items;
        self
    }

    pub fn with_trailing_menu(mut self, items: Vec<MenuItem<Cmd>>) -> Self {
        self.trailing_menu = items;
        self
    }

    pub fn on_click(mut self, command: Cmd) -> Self {
        self.on_click_command = Some(command);
        self
    }
}

// ───────────────────────── Row factory ────────────────────────

pub struct ListItemRow<Cmd: Clone + Debug + Send + 'static> {
    template: ListItem,
    menu_items: Vec<MenuItem<Cmd>>,
    trailing_menu: Vec<MenuItem<Cmd>>,
    on_click_command: Option<Cmd>,
}

impl<Cmd> FactoryComponent for ListItemRow<Cmd>
where
    Cmd: Clone + Debug + Send + 'static,
{
    type Init = ListItemModel<Cmd>;
    type Input = ();
    type Output = Cmd;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;
    type Root = gtk::Box;
    type Widgets = ();
    type Index = DynamicIndex;

    fn init_model(init: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        let template = ListItem::init(());

        template.label.set_label(&init.label);
        template.container.set_tooltip_text(init.tooltip.as_deref());

        if let Some(sub) = init.sublabel.as_deref() {
            template.secondary_label.set_label(sub);
            template.secondary_label.set_visible(true);
        } else {
            template.secondary_label.set_visible(false);
        }

        if let Some(icon_name) = init.icon.as_deref() {
            let image = gtk::Image::from_icon_name(icon_name);
            image.set_pixel_size(16);
            template.left.append(&image);
            template.left.set_visible(true);
        }

        Self {
            template,
            menu_items: init.menu_items,
            trailing_menu: init.trailing_menu,
            on_click_command: init.on_click_command,
        }
    }

    fn init_root(&self) -> Self::Root {
        self.template.container.clone()
    }

    fn init_widgets(
        &mut self,
        _index: &DynamicIndex,
        root: Self::Root,
        _returned: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        sender: FactorySender<Self>,
    ) -> Self::Widgets {
        // Right-click context menu
        if !self.menu_items.is_empty() {
            attach_context_menu(
                &root,
                self.menu_items.clone(),
                sender.output_sender().clone(),
            );
        }

        // Trailing hamburger MenuButton in the right slot
        if !self.trailing_menu.is_empty() {
            let popover = build_menu_popover(&self.trailing_menu, sender.output_sender().clone());
            let button = gtk::MenuButton::new();
            button.set_icon_name("open-menu-symbolic");
            button.set_has_frame(false);
            button.add_css_class("flat");
            button.add_css_class("menu-button");
            button.set_popover(Some(&popover));
            self.template.right.append(&button);
            self.template.right.set_visible(true);
        }

        // Row-body activation. Default Bubble propagation phase means child
        // widgets (like the trailing MenuButton) consume their clicks first;
        // this gesture only fires for clicks that reach the row container.
        if let Some(cmd) = self.on_click_command.clone() {
            let click = gtk::GestureClick::new();
            let output = sender.output_sender().clone();
            click.connect_released(move |_, n_press, _, _| {
                if n_press == 1 {
                    let _ = output.send(cmd.clone());
                }
            });
            root.add_controller(click);
        }
    }
}

// ───────────────────────── ListView component ────────────────

#[derive(Debug)]
pub enum ListViewInput<Cmd> {
    /// Replace all rows.
    SetItems(Vec<ListItemModel<Cmd>>),
}

pub struct ListView<Cmd: Clone + Debug + Send + 'static> {
    rows: FactoryVecDeque<ListItemRow<Cmd>>,
}

#[relm4::component(pub)]
impl<Cmd> SimpleComponent for ListView<Cmd>
where
    Cmd: Clone + Debug + Send + 'static,
{
    type Init = Vec<ListItemModel<Cmd>>;
    type Input = ListViewInput<Cmd>;
    type Output = Cmd;

    view! {
        #[root]
        gtk::Box {
            add_css_class: "list-view",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
            #[local_ref]
            rows_widget -> gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
            },
        }
    }

    fn init(
        items: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut rows = FactoryVecDeque::<ListItemRow<Cmd>>::builder()
            .launch(gtk::Box::new(gtk::Orientation::Vertical, 0))
            .forward(sender.output_sender(), |cmd| cmd);
        {
            let mut guard = rows.guard();
            for item in items {
                guard.push_back(item);
            }
        }
        let model = ListView { rows };
        let rows_widget = model.rows.widget();
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            ListViewInput::SetItems(items) => {
                let mut guard = self.rows.guard();
                guard.clear();
                for item in items {
                    guard.push_back(item);
                }
            }
        }
    }
}
