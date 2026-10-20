use std::collections::HashSet;
use std::sync::LazyLock;

use ammonia::Builder;

use crate::text::hostile;

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

pub fn sanitize_body(body: &str) -> String {
    let bounded: String = body.chars().take(BODY_MAX_CHARS).collect();

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
            _ if hostile(character) => ' ',
            _ => character,
        })
        .collect()
}

#[cfg(test)]
mod tests;
