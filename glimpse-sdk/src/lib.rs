use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Icon {
    None,
    Path { path: String },
    Freedesktop { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub title: String,
    pub subtitle: String,
    pub icon: Icon,
    pub category: String,
}
