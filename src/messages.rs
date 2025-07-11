use crate::commands;

#[derive(Debug)]
pub enum Message {
    Query(String),
    ExecAction(commands::Action),
    Shutdown,
}

#[derive(Debug)]
pub enum UIMessage {
    AddCommand(commands::Command),
    ClearResults,
    ExecAction(commands::Action),
}
