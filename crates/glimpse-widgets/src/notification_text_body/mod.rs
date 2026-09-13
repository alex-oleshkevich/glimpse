mod imp;

use gtk4::glib;

#[cfg(test)]
pub(crate) use imp::BODY_MAX_CHARS;

glib::wrapper! {
    pub struct NotificationTextBody(ObjectSubclass<imp::NotificationTextBody>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for NotificationTextBody {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationTextBody {
    pub fn new() -> Self {
        glib::Object::new()
    }
}

pub(crate) fn plain(markup: &str) -> String {
    let mut stripped = String::with_capacity(markup.len());
    let mut rest = markup;
    while let Some(open) = rest.find('<') {
        stripped.push_str(&rest[..open]);
        rest = match rest[open..].find('>') {
            Some(close) => &rest[open + close + 1..],
            None => "",
        };
    }
    stripped.push_str(rest);
    unescape(&stripped)
}

fn unescape(text: &str) -> String {
    const REFERENCE_MAX_BYTES: usize = 12;
    if !text.contains('&') {
        return text.to_owned();
    }

    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = rest.find(';').filter(|end| *end <= REFERENCE_MAX_BYTES);
        match end.and_then(|end| reference(&rest[1..end]).map(|character| (character, end))) {
            Some((character, end)) => {
                out.push(character);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn reference(name: &str) -> Option<char> {
    if let Some(hex) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
        return u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
    }
    if let Some(decimal) = name.strip_prefix('#') {
        return decimal.parse::<u32>().ok().and_then(char::from_u32);
    }
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some('\u{a0}'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::plain;

    #[test]
    fn refused_markup_reads_as_text() {
        assert_eq!(
            plain(r#"<b>Alice</b>&nbsp;<a href="https://x">said hello</a>"#),
            "Alice\u{a0}said hello"
        );
        assert_eq!(plain("&whoops; &#9733; &#x2605; &amp;"), "&whoops; ★ ★ &");
        assert_eq!(
            plain(&format!("&{}", "é".repeat(6))),
            format!("&{}", "é".repeat(6))
        );
    }
}
