use std::collections::HashSet;
use std::sync::LazyLock;

use ammonia::Builder;

pub const BODY_MAX_CHARS: usize = 2048;

const NBSP: &str = "&nbsp;";

static SANITIZER: LazyLock<Builder<'static>> = LazyLock::new(|| {
    let tags: HashSet<&'static str> = ["b", "i", "u"].into_iter().collect();
    let mut builder = Builder::default();
    builder.tags(tags).link_rel(None).strip_comments(true);
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

    SANITIZER
        .clean(&bounded)
        .to_string()
        .replace(NBSP, "\u{a0}")
}

#[cfg(test)]
mod tests;
