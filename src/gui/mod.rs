use iced::{window::Level, *};

use crate::gui::app::GuiApp;

pub mod app;
pub mod messages;
pub mod widgets;

pub fn run() -> iced::Result {
    iced::application("Glimpse", GuiApp::update, GuiApp::view)
        .theme(|_| Theme::CatppuccinLatte)
        .centered()
        .decorations(false)
        .level(Level::AlwaysOnTop)
        .resizable(false)
        .exit_on_close_request(std::env::var("GLIMPSE_DEBUG_CLOSE_ON_CLOSE").is_ok())
        .window_size(Size::new(700.0, 500.0))
        .subscription(GuiApp::subscription)
        .run_with(GuiApp::new)
}
