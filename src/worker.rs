use std::sync::mpsc;

use gio::prelude::*;
use gtk::gdk::prelude::*;

use crate::{commands, messages, search::Search};

pub struct Worker {
    search: Search,
}

impl Worker {
    pub fn new(search: Search) -> Self {
        Self { search }
    }

    pub fn run(
        &self,
        receiver: mpsc::Receiver<messages::Message>,
        sender: async_channel::Sender<messages::UIMessage>,
    ) {
        while let Ok(message) = receiver.recv() {
            match message {
                messages::Message::Query(query) => {
                    let mut cleared = false;
                    let commands = self.search.search(&query);
                    for command in commands {
                        if !cleared {
                            sender
                                .send_blocking(messages::UIMessage::ClearResults)
                                .expect("Failed to send clear results message");
                            cleared = true;
                        }

                        sender
                            .send_blocking(messages::UIMessage::AddCommand(command))
                            .expect("Failed to send command to UI thread");
                    }
                }
                messages::Message::ExecAction(action) => match action {
                    commands::Action::LaunchApp { app_id } => {
                        gio::DesktopAppInfo::new(&app_id)
                            .expect("Failed to create DesktopAppInfo")
                            .launch(&[], None::<&gio::AppLaunchContext>)
                            .expect("Failed to launch app");
                    }
                    commands::Action::CopyToClipboard(text) => {
                        let display =
                            gtk::gdk::Display::default().expect("Failed to get default display");
                        let clipboard = display.clipboard();
                        clipboard.set_text(&text);
                    }
                    commands::Action::ShellExec(command, args) => {
                        let output = std::process::Command::new(command)
                            .args(args)
                            .spawn()
                            .expect("Failed to start command");
                    }
                    commands::Action::Noop => {}
                    _ => {
                        eprintln!("Unsupported action: {:?}", action);
                    }
                },
                messages::Message::Shutdown => {
                    break;
                }
            }
        }
    }
}
