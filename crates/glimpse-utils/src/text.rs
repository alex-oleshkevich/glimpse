/// Bidi overrides and isolates, which reorder the characters after them and let a feed render
/// `gpj.exe` as `exe.jpg` inside a label that is doing nothing wrong.
fn hostile(character: char) -> bool {
    character.is_control() || matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

/// Flattens whitespace and drops hostile characters, then caps by character count rather than by
/// byte, so a multi-byte string cannot be cut mid-codepoint.
pub fn clean(text: &str, cap: usize) -> String {
    let mut cleaned = String::new();
    let mut length = 0;
    let mut spaced = false;

    for character in text.chars() {
        if character.is_whitespace() || hostile(character) {
            spaced = length > 0;
            continue;
        }
        if length >= cap {
            cleaned.push('…');
            break;
        }
        if spaced {
            cleaned.push(' ');
            length += 1;
            spaced = false;
        }
        cleaned.push(character);
        length += 1;
    }
    cleaned
}

#[cfg(test)]
mod tests {
    use super::clean;

    #[test]
    fn text_off_a_feed_is_flattened_and_capped() {
        assert_eq!(clean("  Design\treview\n\n", 120), "Design review");
        assert_eq!(
            clean("a\u{7}b", 120),
            "a b",
            "a control character separates rather than vanishes, so it cannot splice two words"
        );
        assert_eq!(
            clean("Lunch\u{202e}gpj.exe", 120),
            "Lunch gpj.exe",
            "a bidi override is not a control character, and Pango honours it: left in, it \
             reorders the row it lands in"
        );
        assert_eq!(clean("a\u{2066}b\u{2069}c", 120), "a b c");
        assert_eq!(clean("abcdef", 4), "abcd…");
        assert_eq!(
            clean("ééééé", 3),
            "ééé…",
            "the cap counts characters, and slicing bytes would panic here"
        );
        assert_eq!(clean("   ", 120), "");
    }
}
