use tracing_subscriber::FmtSubscriber;

use crate::app::App;

mod app;
mod extensions;
mod gui;
mod search;

fn main() -> iced::Result {
    init_logging();

    let mut app = App::new();
    app.initialize();

    gui::run(app)
}

fn init_logging() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::DEBUG)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}
