use crate::commands;
use crate::extensions;
use crate::messages;

pub struct Executor<'a> {
    extensions: Vec<&'a dyn extensions::Extension>,
}

impl<'a> Executor<'a> {
    pub fn new(extensions: Vec<&'a dyn extensions::Extension>) -> Self {
        Self { extensions }
    }

    pub fn query(&self, query: &messages::Message) -> Vec<commands::Command> {
        let mut results = Vec::new();
        for extension in &self.extensions {
            results.extend(extension.query(query));
        }
        results
    }
}
