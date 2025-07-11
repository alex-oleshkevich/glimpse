use std::sync::Arc;

use crate::{commands, extensions, messages};

pub struct Search {
    extensions: Arc<extensions::ExtensionManager>,
}

impl Search {
    pub fn new(extensions: Arc<extensions::ExtensionManager>) -> Self {
        Self { extensions }
    }

    pub fn search(&self, query: &str) -> Vec<commands::Command> {
        let mut results = Vec::new();
        for extension in self.extensions.all() {
            let commands = extension.query(&messages::Message::Query(query.to_string()));
            for command in commands {
                results.push(command);
            }
        }
        results
    }

    // pub fn run(&self, result_sender: async_channel::Sender<messages::UIMessage>) {
    //     for message in self.command_receiver.iter() {
    //         match message {
    //             messages::Message::Query(query) => {
    //                 let mut cleared = false;
    //                 for extension in self.extensions.all() {
    //                     let commands = extension.query(&messages::Message::Query(query.clone()));
    //                     for command in commands {
    //                         if !cleared {
    //                             result_sender
    //                                 .send_blocking(messages::UIMessage::ClearResults)
    //                                 .expect("Failed to clear results in UI thread");
    //                             cleared = true;
    //                         }

    //                         result_sender
    //                             .send_blocking(messages::UIMessage::AddCommand(command))
    //                             .expect("Failed to send command to UI thread");
    //                     }
    //                 }
    //             }
    //             messages::Message::Shutdown => {
    //                 break;
    //             }
    //         }
    //     }
    // }
}
