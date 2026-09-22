use glimpse_services::{ClipboardEntry, ClipboardKind};
use glimpse_utils::text::clean;

use crate::applets::tokens;

pub const ICON: &str = "edit-paste-symbolic";
pub const TEXT_ICON: &str = "text-x-generic-symbolic";
pub const IMAGE_ICON: &str = "image-x-generic-symbolic";

/// What a text row's title reads: the entry's own preview, capped again to the configured width.
/// The service already bounded and cleaned it; this is the second cap the untrusted-text rule asks
/// for. An image has no preview to cap — the applet words that row from its size instead.
pub fn title(entry: &ClipboardEntry, cap: usize) -> String {
    clean(&entry.preview, cap)
}

pub fn size(bytes: usize) -> String {
    const KIB: f64 = 1024.0;
    let bytes = bytes as f64;
    match bytes {
        b if b < KIB => format!("{} B", b as usize),
        b if b < KIB * KIB => format!("{:.0} kB", b / KIB),
        b => format!("{:.1} MB", b / (KIB * KIB)),
    }
}

/// Whether the bar shows a chip at all.
///
/// An empty history takes no room, but **a clipboard that cannot be watched must still be
/// reachable**: with no data-control protocol nothing is ever captured, so a rule that hid the chip
/// on an empty history would hide the one surface that explains why it is empty, for ever.
pub fn shown(count: usize, show_when_empty: bool, unavailable: bool) -> bool {
    count > 0 || show_when_empty || unavailable
}

/// The chip's label and tooltip, through the same token substitution every other applet uses.
pub fn filled(template: Option<&str>, count: usize) -> Option<String> {
    let template = template?;
    let count = count.to_string();
    Some(tokens::render(template, |token| match token {
        "count" => Some(count.as_str()),
        _ => None,
    }))
}

pub fn icon_for(kind: ClipboardKind) -> &'static str {
    match kind {
        ClipboardKind::Text => TEXT_ICON,
        ClipboardKind::Image => IMAGE_ICON,
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;

    use super::*;

    #[test]
    fn a_title_is_capped_by_character_and_survives_multi_byte_text() {
        let entry = text_entry(&"Марта🙂".repeat(50));

        let rendered = title(&entry, 12);

        assert!(rendered.chars().count() <= 13, "got {rendered}");
        assert!(rendered.starts_with("Марта🙂"));
    }

    #[test]
    fn sizes_step_through_bytes_kilobytes_and_megabytes() {
        assert_eq!(size(58), "58 B");
        assert_eq!(size(2048), "2 kB");
        assert_eq!(size(1_468_006), "1.4 MB");
    }

    #[test]
    fn an_empty_history_takes_no_room_on_the_bar() {
        assert!(!shown(0, false, false));
        assert!(shown(1, false, false));
        assert!(shown(0, true, false));
    }

    /// Without this the warning is unreachable: nothing is ever captured, so the count stays zero,
    /// so the chip stays hidden, so the popover that would explain it can never be opened.
    #[test]
    fn a_clipboard_that_cannot_be_watched_is_still_reachable() {
        assert!(
            shown(0, false, true),
            "a compositor with no data-control protocol must still get a chip to press"
        );
    }

    #[test]
    fn a_tooltip_without_a_template_is_absent_rather_than_empty() {
        assert_eq!(filled(None, 4), None);
        assert_eq!(filled(Some("{count} items"), 4).as_deref(), Some("4 items"));
    }

    /// An unknown token is left as written rather than swallowed, so a typo in the document is
    /// visible on the bar instead of silently producing an empty chip.
    #[test]
    fn an_unknown_token_survives_into_the_label() {
        assert_eq!(
            filled(Some("{count}/{nonesuch}"), 2).as_deref(),
            Some("2/{nonesuch}")
        );
    }

    fn text_entry(body: &str) -> ClipboardEntry {
        ClipboardEntry {
            id: 1,
            kind: ClipboardKind::Text,
            mime: "text/plain".to_owned(),
            preview: body.to_owned(),
            pinned: false,
            at: DateTime::from_timestamp(1_700_000_000, 0).expect("an instant"),
            data: std::sync::Arc::from(body.as_bytes()),
        }
    }
}
