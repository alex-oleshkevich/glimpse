use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Share your calendar events between the clock grid, day list and `next-event`.
/// You get events from 31 days back to 62 days ahead; browsing the clock moves that window, with a maximum span of 400 days.
/// Up to 512 events are kept, with 512 occurrences per recurring event and titles cut at 120 characters.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Calendar {
    /// How often online calendars update, in seconds, unless a source sets its own interval.
    /// Local calendars update when their files change.
    pub poll_interval: u64,
    /// The calendars you read events from.
    pub sources: Vec<Source>,
}

impl Default for Calendar {
    fn default() -> Self {
        Self {
            poll_interval: 600,
            sources: Vec::new(),
        }
    }
}

/// One calendar to read.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Source {
    /// A unique name for this calendar source.
    pub id: String,
    /// Choose `ical` for a calendar file or online feed, or `directory` for a folder of `.ics`
    /// files.
    #[serde(rename = "type")]
    pub kind: SourceKind,
    /// Your calendar feed URL, local file or folder. To keep a secret feed URL out of shared
    /// config, use a `file://` path to a file containing the URL on one line.
    pub uri: String,
    /// What to call this calendar on screen. Unset falls back to `id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// How often this online calendar updates, in seconds, instead of using the shared
    /// interval.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_interval: Option<u64>,
    /// The color of the dot beside this calendar's events, such as `purple` or `#336699`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// What a source's `uri` points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    /// One iCalendar document: an `https://` or `webcal://` feed, a local `.ics`, or a `file://`
    /// holding a one-line feed URL. A local one is watched; a remote one is fetched on
    /// `poll-interval`.
    Ical,
    /// A folder of `.ics` calendar files.
    Directory,
}
