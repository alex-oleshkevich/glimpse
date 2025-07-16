use iced::{window::Level, *};

use crate::app::App;

pub mod app;
pub mod widgets;

pub fn run(app: App) -> iced::Result {
    iced::application("Glimpse", app::update, app::view)
        .theme(|_| Theme::CatppuccinLatte)
        .centered()
        .decorations(false)
        .level(Level::AlwaysOnTop)
        .resizable(false)
        .exit_on_close_request(std::env::var("GLIMPSE_DEBUG_CLOSE_ON_CLOSE").is_ok())
        .window_size(Size::new(700.0, 500.0))
        .subscription(app::subscription)
        .run_with(|| (app::State::new(app.channel), Task::none()))
}
