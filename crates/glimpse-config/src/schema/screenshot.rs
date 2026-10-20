use std::fmt::Write as _;
use std::path::PathBuf;

use chrono::format::{Item, StrftimeItems};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

const MAX_DELAY: u32 = 60;

/// Choose where you save screenshots and how your editor opens.
/// The `[screenshot]` table in `shot.toml` beside your active config overrides these settings one key at a time; other tables there are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Screenshot {
    /// Where you save screenshots, with the folder created on your first save. Use a full path; `~`
    /// doesn't expand to your home folder.
    pub directory: Option<PathBuf>,
    /// The file name, as a strftime pattern expanded at the capture time. Must end in `.png` and
    /// contain no `/`.
    pub filename: FileTemplate,
    /// Where an annotation opens: over the screen, or in a window.
    pub editor: Editor,
    /// How many seconds you wait before an editor capture, from 0 to 60. The `glimpse-shot capture`
    /// command uses its own `--delay` option.
    #[serde(deserialize_with = "delay")]
    pub delay: u32,
    /// Flash your screen when the editor takes a screenshot. The `glimpse-shot capture` command
    /// doesn't flash.
    pub flash: bool,
    /// Include the pointer in a capture.
    pub cursor: bool,
    /// Notify you after saving or copying a screenshot from the fullscreen editor.
    pub notify: bool,
    /// The color annotation tools start with.
    pub color: PaletteColor,
    /// The starting thickness of your annotation lines.
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
        let mut sample = String::new();
        let _ = write!(
            sample,
            "{}",
            DateTime::<Utc>::UNIX_EPOCH.format_with_items(StrftimeItems::new(&template))
        );
        if sample.contains('/') {
            return Err("filename must not contain `/`".to_owned());
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
