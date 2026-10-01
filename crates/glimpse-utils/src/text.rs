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
        if length + usize::from(spaced) >= cap {
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
mod tests;
