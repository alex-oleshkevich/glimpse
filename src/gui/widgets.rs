use std::path;

use crate::gui::app::{Message, Screen, SearchMessage};
use crate::search::{Icon, SearchItem};
use iced::widget::{
    Button, Space, button, column, container, row, scrollable, svg, text, text_input,
};
use iced::*;

pub fn main_view<'a>(query: &'a String, search_items: &'a Vec<SearchItem>) -> Element<'a, Message> {
    column![
        container(
            text_input("Enter title", query.as_ref())
                .on_input(|title| Message::Search(SearchMessage::StartSearch(title)))
                .padding(12)
        )
        .width(Length::Fill)
        .padding(12),
        container(scrollable(search_list(&search_items)))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(12)
    ]
    .into()
}

pub fn plugin_view(items: &Vec<SearchItem>) -> Element<Message> {
    column![
        text("Plugin View"),
        button("Back to Main").on_press(Message::Navigate(Screen::Search)),
    ]
    .into()
}

pub fn search_icon(icon: &Icon) -> Element<Message> {
    match &icon {
        Icon::Path(path) => search_icon_from_path(path),
    }
}

pub fn search_icon_from_path(path: &str) -> Element<Message> {
    let handle = svg::Handle::from_path(path::PathBuf::from(path));
    container(svg(handle)).width(40).height(40).into()
}

pub fn row_actions() -> Element<'static, Message> {
    row![
        button("Action 1").on_press(Message::Navigate(Screen::Search)),
        button("Action 2").on_press(Message::Navigate(Screen::PluginView)),
    ]
    .into()
}

pub fn search_item(item: &SearchItem) -> Element<Message> {
    let mut row = Button::new(
        row![
            container(search_icon(&item.icon)).padding(4),
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

    if let Some(action) = item.primary_action() {
        row = row.on_press(Message::DispatchAction(action.clone()));
    }
    row.into()
}

pub fn search_list(items: &Vec<SearchItem>) -> Element<Message> {
    column(items.iter().map(search_item))
        .padding(12)
        .width(Length::Fill)
        .into()
}
