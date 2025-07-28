use tracing_subscriber::FmtSubscriber;

mod app;
mod bridge;
mod extensions;
mod gui;
mod icons;
mod jsonrpc;
mod search;

fn main() -> iced::Result {
    init_logging();

    gui::run()
}

fn init_logging() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::DEBUG)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}
