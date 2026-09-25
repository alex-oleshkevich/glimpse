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
mod tests {
    use super::*;

    fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, text).expect("writes");
        crate::load(Some(&path))
    }

    #[test]
    fn an_absent_table_uses_the_documented_defaults() {
        let screenshot = load("").expect("an absent table is fine").screenshot;

        assert_eq!(screenshot.directory, None);
        assert_eq!(
            screenshot.filename.as_str(),
            "Screenshot from %Y-%m-%d %H-%M-%S.png"
        );
        assert_eq!(screenshot.editor, Editor::Overlay);
        assert_eq!(screenshot.delay, 0);
        assert!(screenshot.flash);
        assert!(!screenshot.cursor);
        assert!(screenshot.notify);
        assert_eq!(screenshot.color, PaletteColor::Red);
        assert_eq!(screenshot.stroke, Stroke::Medium);
        assert_eq!(Stroke::Medium as u8, 2);
    }

    #[test]
    fn every_key_reads_kebab_case() {
        let screenshot = load(
            "[screenshot]\ndirectory = \"/tmp/shots\"\nfilename = \"shot %s.png\"\n\
             editor = \"window\"\ndelay = 5\nflash = false\ncursor = true\nnotify = false\n\
             color = \"blue\"\nstroke = \"thick\"\n",
        )
        .expect("every key")
        .screenshot;

        assert_eq!(screenshot.directory, Some(PathBuf::from("/tmp/shots")));
        assert_eq!(screenshot.filename.as_str(), "shot %s.png");
        assert_eq!(screenshot.editor, Editor::Window);
        assert_eq!(screenshot.delay, 5);
        assert!(!screenshot.flash && screenshot.cursor && !screenshot.notify);
        assert_eq!(screenshot.color, PaletteColor::Blue);
        assert_eq!(screenshot.stroke, Stroke::Thick);
    }

    #[test]
    fn the_default_filename_loads() {
        let default = Screenshot::default().filename.as_str().to_owned();
        let document = format!("[screenshot]\nfilename = \"{default}\"\n");

        assert_eq!(
            load(&document).expect("the default loads").screenshot,
            Screenshot::default()
        );
    }

    #[test]
    fn a_bad_filename_is_refused() {
        for (filename, reason) in [
            ("shot %Q.png", "strftime"),
            ("shots/%s.png", "`/`"),
            ("shot %s.jpg", "`.png`"),
        ] {
            let error = load(&format!("[screenshot]\nfilename = \"{filename}\"\n"))
                .expect_err("a bad filename is refused")
                .to_string();
            assert!(error.contains(reason), "{filename}: {error}");
        }
    }

    #[test]
    fn a_delay_over_a_minute_is_clamped() {
        assert_eq!(
            load("[screenshot]\ndelay = 99\n")
                .expect("clamped")
                .screenshot
                .delay,
            60
        );
    }

    #[test]
    fn an_unknown_key_is_refused_naming_the_table_and_the_key() {
        let rendered = load("[screenshot]\nbogus = 1\n")
            .expect_err("`bogus` is not a screenshot setting")
            .to_string();

        assert!(
            rendered.contains("screenshot") && rendered.contains("bogus"),
            "the error must name the table and the key, got {rendered}"
        );
    }
}
