pub struct Extension {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
}

pub struct ExtensionManager {
    extensions: Vec<Extension>,
}

impl ExtensionManager {
    pub fn new() -> Self {
        ExtensionManager {
            extensions: Vec::new(),
        }
    }

    pub fn load_extensions(&mut self) {}
}
