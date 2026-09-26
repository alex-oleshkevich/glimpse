use glimpse_services::{ClipboardEntry, ClipboardKind};
use glimpse_utils::text::clean;

use crate::applets::tokens;

pub const ICON: &str = "edit-paste-symbolic";
pub const TEXT_ICON: &str = "text-x-generic-symbolic";
pub const IMAGE_ICON: &str = "image-x-generic-symbolic";
pub const LINK_ICON: &str = "insert-link-symbolic";
pub const PATH_ICON: &str = "folder-symbolic";

/// What a text row's title reads: the entry's own preview, capped again to the configured width.
/// The service already bounded and cleaned it; this is the second cap the untrusted-text rule asks
/// for. An image has no preview to cap — the applet words that row from its size instead.
pub fn title(entry: &ClipboardEntry, cap: usize) -> String {
    clean(&entry.preview, cap)
}

pub fn title_of(text: &str, cap: usize) -> String {
    clean(text, cap)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    Link {
        url: String,
        host: String,
        rest: String,
    },
    Color([u8; 3]),
    Path(String),
    Lines(usize),
    Plain,
}

pub fn shape(text: &str) -> Shape {
    let trimmed = text.trim();
    if trimmed.lines().count() > 1 {
        return Shape::Lines(trimmed.lines().count());
    }
    if let Some(color) = hex(trimmed) {
        return Shape::Color(color);
    }
    if trimmed.starts_with('/') || trimmed.starts_with("~/") {
        return Shape::Path(trimmed.to_owned());
    }
    if !trimmed.contains(char::is_whitespace)
        && let Ok(url) = url::Url::parse(trimmed)
        && matches!(url.scheme(), "http" | "https")
        && let Some(host) = url.host_str()
    {
        let rest = url[url::Position::BeforePath..]
            .trim_start_matches('/')
            .to_owned();
        return Shape::Link {
            url: url.to_string(),
            host: host.trim_start_matches("www.").to_owned(),
            rest,
        };
    }
    Shape::Plain
}

fn hex(text: &str) -> Option<[u8; 3]> {
    let digits = text.strip_prefix('#')?;
    if !digits.chars().all(|digit| digit.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |at: usize, width: usize| {
        let part = &digits[at..at + width];
        u8::from_str_radix(part, 16)
            .ok()
            .map(|value| if width == 1 { value * 17 } else { value })
    };
    match digits.len() {
        3 => Some([channel(0, 1)?, channel(1, 1)?, channel(2, 1)?]),
        6 => Some([channel(0, 2)?, channel(2, 2)?, channel(4, 2)?]),
        _ => None,
    }
}

pub fn excerpt(text: &str, lines: usize, cap: usize) -> String {
    text.trim()
        .lines()
        .take(lines)
        .map(|line| clean(line, cap))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn home_path(path: &str, home: &std::path::Path) -> std::path::PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => std::path::PathBuf::from(path),
    }
}

pub fn matches(entry: &ClipboardEntry, query: &str) -> bool {
    let query = query.trim();
    query.is_empty()
        || (entry.kind == ClipboardKind::Text
            && entry.preview.to_lowercase().contains(&query.to_lowercase()))
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

    #[test]
    fn a_web_address_reads_as_its_path_under_its_host() {
        assert_eq!(
            shape(" https://www.github.com/niri-wm/niri/pull/2141 "),
            Shape::Link {
                url: "https://www.github.com/niri-wm/niri/pull/2141".to_owned(),
                host: "github.com".to_owned(),
                rest: "niri-wm/niri/pull/2141".to_owned(),
            }
        );
        assert_eq!(shape("ftp://example.com/file"), Shape::Plain);
        assert_eq!(shape("see https://example.com"), Shape::Plain);
    }

    #[test]
    fn a_hex_color_is_recognized_in_both_lengths_and_nothing_else_is() {
        assert_eq!(shape("#1e88e5"), Shape::Color([30, 136, 229]));
        assert_eq!(shape("#fff"), Shape::Color([255, 255, 255]));
        assert_eq!(shape("#12345"), Shape::Plain);
        assert_eq!(shape("#ggg"), Shape::Plain);
        assert_eq!(shape("#"), Shape::Plain);
    }

    #[test]
    fn paths_and_several_lines_are_told_apart() {
        assert_eq!(shape("~/notes.txt"), Shape::Path("~/notes.txt".to_owned()));
        assert_eq!(shape("/etc/hosts"), Shape::Path("/etc/hosts".to_owned()));
        assert_eq!(shape("one\ntwo\nthree"), Shape::Lines(3));
        assert_eq!(shape("just words"), Shape::Plain);
        assert_eq!(
            home_path("~/notes.txt", std::path::Path::new("/home/me")),
            std::path::PathBuf::from("/home/me/notes.txt")
        );
    }

    #[test]
    fn an_excerpt_keeps_lines_but_cleans_and_caps_each() {
        assert_eq!(
            excerpt("  first\u{202e}\n\tsecond line\nthird\n", 2, 5),
            "first\nsecon…"
        );
    }

    #[test]
    fn a_search_matches_text_without_case_and_never_an_image() {
        assert!(matches(&text_entry("Code Review"), "review"));
        assert!(matches(&text_entry("anything"), "  "));
        assert!(!matches(&text_entry("anything"), "else"));
        let mut image = text_entry("");
        image.kind = ClipboardKind::Image;
        assert!(!matches(&image, "png"));
        assert!(matches(&image, ""));
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
