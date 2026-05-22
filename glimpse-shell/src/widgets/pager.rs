use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PagerAppearance {
    #[default]
    Dots,
    Numbers,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PagerItemView {
    pub appearance: PagerAppearance,
    pub label: String,
    pub active: bool,
    pub inactive: bool,
    pub occupied: bool,
    pub urgent: bool,
}
