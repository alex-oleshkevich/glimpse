use std::collections::HashMap;

use crate::{commands, messages};

pub struct ExtensionMetadata {
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
}

pub trait Extension: Send + Sync {
    fn id(&self) -> String;
    fn metadata(&self) -> ExtensionMetadata;
    fn query(&self, query: &messages::Message) -> Vec<commands::Command>;
}

pub struct ExtensionManager {
    extensions: HashMap<String, Box<dyn Extension>>,
}

impl ExtensionManager {
    pub fn new() -> Self {
        Self {
            extensions: HashMap::new(),
        }
    }

    pub fn load_extensions(&mut self) {
        let apps = Box::new(crate::contrib::apps::Apps::new());
        self.extensions.insert(apps.id(), apps);

        let calculator = Box::new(crate::contrib::calculator::Calculator::new());
        self.extensions.insert(calculator.id(), calculator);

        let shell = Box::new(crate::contrib::shell::Shell::new());
        self.extensions.insert(shell.id(), shell);
    }

    pub fn get(&self, id: &str) -> Option<&Box<dyn Extension>> {
        self.extensions.get(id)
    }

    pub fn all(&self) -> Vec<&dyn Extension> {
        self.extensions.values().map(|ext| ext.as_ref()).collect()
    }
}
