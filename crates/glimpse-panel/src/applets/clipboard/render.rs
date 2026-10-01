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
mod tests;
