use std::path::PathBuf;

use chrono::format::{Item, StrftimeItems};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

const MAX_DELAY: u32 = 60;

/// Screenshots. `glimpse-shot` reads it for where a capture is saved and how the editor opens.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Screenshot {
    /// Where a capture is saved, created on first save. Unset means `$XDG_PICTURES_DIR/Screenshots`.
    pub directory: Option<PathBuf>,
    /// The file name, as a strftime pattern expanded at the capture time. Must end in `.png` and
    /// contain no `/`.
    pub filename: FileTemplate,
    /// Where an annotation opens: over the screen, or in a window.
    pub editor: Editor,
    /// Seconds to wait before capturing. Clamped to 0..=60.
    #[serde(deserialize_with = "delay")]
    pub delay: u32,
    /// Flash the screen on capture.
    pub flash: bool,
    /// Include the pointer in a capture.
    pub cursor: bool,
    /// Post a notification after a capture is saved.
    pub notify: bool,
    /// The color annotation tools start with.
    pub color: PaletteColor,
    /// The stroke width annotation tools start with.
    pub stroke: Stroke,
}

impl Default for Screenshot {
    fn default() -> Self {
        Self {
            directory: None,
            filename: FileTemplate("Screenshot from %Y-%m-%d %H-%M-%S.png".to_owned()),
            editor: Editor::Overlay,
            delay: 0,
            flash: true,
            cursor: false,
            notify: true,
            color: PaletteColor::Red,
            stroke: Stroke::Medium,
        }
    }
}

/// A strftime pattern naming a saved capture.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(try_from = "String")]
pub struct FileTemplate(String);

impl FileTemplate {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for FileTemplate {
    type Error = String;

    fn try_from(template: String) -> Result<Self, Self::Error> {
        if !template.ends_with(".png") {
            return Err("filename must end in `.png`".to_owned());
        }
        if template.contains('/') {
            return Err("filename must not contain `/`".to_owned());
        }
        if StrftimeItems::new(&template).any(|item| matches!(item, Item::Error)) {
            return Err("filename is not a valid strftime pattern".to_owned());
        }
        Ok(Self(template))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Editor {
    Overlay,
    Window,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PaletteColor {
    Red,
    Yellow,
    Green,
    Blue,
    White,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Stroke {
    Thin = 1,
    Medium = 2,
    Thick = 3,
}

fn delay<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(u32::deserialize(deserializer)?.min(MAX_DELAY))
}

#[cfg(test)]
mod tests;
