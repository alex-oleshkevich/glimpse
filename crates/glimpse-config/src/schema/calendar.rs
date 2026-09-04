use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Where calendar events come from. One list, read by every surface that shows an event: the
/// clock popover, the day markers on its grid, and the `next-event` applet.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Calendar {
    /// How often a source is read again, in seconds, unless it names its own.
    pub poll_interval: u64,
    /// The sources themselves. Every one listed is active; remove it to stop reading it.
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
    /// What this source is called internally. It has to be unique, and it is what an event
    /// carries so a surface can tell which calendar it came from.
    pub id: String,
    /// Whether `uri` names a subscription feed or a directory of `.ics` files.
    #[serde(rename = "type")]
    pub kind: SourceKind,
    /// The feed or directory. A provider's iCalendar URL behaves like a read-only access
    /// token — anyone holding it can usually read the calendar without signing in — so a
    /// `file://` path to a one-line sidecar file is offered as an alternative to writing a
    /// secret URL into a configuration file that gets shared or committed.
    pub uri: String,
    /// What to call this calendar on screen. Unset falls back to `id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// How often to read this one, in seconds, overriding the shared interval.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_interval: Option<u64>,
    /// The dot beside this calendar's events, as a hex color.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// What a source's `uri` points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    /// An iCalendar subscription feed, over `https://` or `file://`.
    Ical,
    /// A directory of `.ics` files.
    Directory,
}
