use std::collections::HashSet;
use std::sync::LazyLock;

use ammonia::Builder;

pub const BODY_MAX_CHARS: usize = 2048;
pub const BODY_MAX_LINES: usize = 3;

const NBSP: &str = "&nbsp;";
const BR: &str = "<br>";

static SANITIZER: LazyLock<Builder<'static>> = LazyLock::new(|| {
    let tags: HashSet<&'static str> = ["b", "i", "u", "br"].into_iter().collect();
    let mut builder = Builder::default();
    builder
        .tags(tags)
        .generic_attributes(HashSet::new())
        .link_rel(None)
        .strip_comments(true);
    builder
});

fn hostile(character: char) -> bool {
    (character.is_control() && character != '\n')
        || matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

pub fn sanitize_body(body: &str) -> String {
    let bounded: String = body
        .chars()
        .take(BODY_MAX_CHARS)
        .map(|character| match hostile(character) {
            true => ' ',
            false => character,
        })
        .collect();

    let clean = SANITIZER
        .clean(&bounded)
        .to_string()
        .replace(NBSP, "\u{a0}")
        .replace(BR, "\n");
    let mut breaks = 0;
    clean
        .chars()
        .map(|character| match character {
            '\n' if breaks + 1 < BODY_MAX_LINES => {
                breaks += 1;
                character
            }
            '\n' => ' ',
            _ => character,
        })
        .collect()
}

#[cfg(test)]
mod tests;
