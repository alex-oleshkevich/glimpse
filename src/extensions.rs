use crate::{commands, messages};

pub struct ExtensionMetadata {
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
}

pub trait Extension: Send + Sync {
    fn metadata(&self) -> ExtensionMetadata;
    fn query(&self, query: &messages::Message) -> Vec<commands::Command>;
    fn execute(&self, action: &commands::Action) -> Result<Vec<commands::Command>, String>;
}

pub struct ExtensionManager {
    extensions: Vec<Box<dyn Extension>>,
}

impl ExtensionManager {
    pub fn new() -> Self {
        Self {
            extensions: Vec::new(),
        }
    }

    pub fn load_extensions(&mut self) {
        self.extensions
            .push(Box::new(crate::contrib::apps::Apps::new()));
    }

    pub fn all(&self) -> Vec<&dyn Extension> {
        self.extensions.iter().map(|ext| ext.as_ref()).collect()
    }
}
