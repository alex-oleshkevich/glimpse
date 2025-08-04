use std::path;

use freedesktop_icons::lookup;
use glimpse_sdk::{Command, Icon};
use iced::{
    Color, Element, Length,
    widget::{Button, Space, button, column, container, row, scrollable, svg, text, text_input},
};

use crate::{
    app::State,
    messages::{Message, Screen},
};

pub fn main_view<'a>(state: &'a State) -> Element<'a, Message> {
    column![
        container(
            text_input("Enter title", &state.query)
                .on_input(|text| Message::Search(text))
                .padding(12)
        )
        .width(Length::Fill)
        .padding(12),
        container(scrollable(search_list(&state.search_items)))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(12)
    ]
    .into()
}

pub fn plugin_view(items: &Vec<Command>) -> Element<'static, Message> {
    column![
        text("Plugin View"),
        button("Back to Main").on_press(Message::Navigate(Screen::MainView)),
    ]
    .into()
}

pub fn search_list(items: &Vec<Command>) -> Element<Message> {
    column(items.iter().map(search_item))
        .padding(12)
        .width(Length::Fill)
        .into()
}

pub fn search_item(item: &Command) -> Element<Message> {
    let row = Button::new(
        row![
            // container(search_icon(&item.icon)).padding(4),
            container(column![
                text(&item.title).size(20),
                text(&item.subtitle).size(16)
            ])
            .padding(4),
            Space::with_width(Length::Fill),
            container(text(&item.category).size(14)).padding(4),
        ]
        .width(Length::Fill),
    )
    .style(|_, _| button::Style::default().with_background(Color::TRANSPARENT));

    // if let Some(action) = item.primary_action() {
    //     row = row.on_press(Message::DispatchAction(action.clone()));
    // }
    row.into()
}

pub fn row_actions() -> Element<'static, Message> {
    row![
        button("Action 1").on_press(Message::Navigate(Screen::MainView)),
        button("Action 2").on_press(Message::Navigate(Screen::PluginView)),
    ]
    .into()
}

pub fn search_icon(icon: &Icon) -> Element<Message> {
    match icon {
        Icon::None => container(text("No Icon")).width(40).height(40).into(),
        Icon::Path { path } => search_icon_from_path(path),
        Icon::Freedesktop { name } => search_icon_from_name(name),
    }
}

pub fn search_icon_from_path(path: &str) -> Element<Message> {
    let handle = svg::Handle::from_path(path::PathBuf::from(path));
    container(svg(handle)).width(40).height(40).into()
}

pub fn search_icon_from_name(name: &str) -> Element<Message> {
    let icon = lookup(name).find();

    match icon {
        Some(icon) => {
            return container(svg(icon)).width(40).height(40).into();
        }
        None => {
            return container(text(name)).width(40).height(40).into();
        }
    }
}
