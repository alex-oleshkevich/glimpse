use crate::commands;

#[derive(Debug)]
pub enum Message {
    Query(String),
}

#[derive(Debug)]
pub enum UIMessage {
    AddCommand(commands::Command),
    ClearResults,
    ExecAction(commands::Command, commands::Action),
}
