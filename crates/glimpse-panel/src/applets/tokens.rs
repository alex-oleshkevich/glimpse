pub fn render<'a>(template: &str, resolve: impl Fn(&str) -> Option<&'a str>) -> String {
    let mut rendered = String::new();
    let mut rest = template;

    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        rendered.push_str(&rest[..open]);
        let token = &rest[open + 1..open + close];
        match resolve(token) {
            Some(value) => rendered.push_str(value),
            None => {
                rendered.push('{');
                rendered.push_str(token);
                rendered.push('}');
            }
        }
        rest = &rest[open + close + 1..];
    }

    rendered.push_str(rest);
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(token: &str) -> Option<&'static str> {
        match token {
            "name" => Some("{id}"),
            "id" => Some("42"),
            _ => None,
        }
    }

    #[test]
    fn text_around_a_token_survives() {
        assert_eq!(render("ws {id}!", facts), "ws 42!");
        assert_eq!(render("", facts), "");
        assert_eq!(render("no tokens", facts), "no tokens");
    }

    #[test]
    fn an_unknown_token_is_left_alone_rather_than_silently_emptied() {
        assert_eq!(render("{nonesuch}", facts), "{nonesuch}");
    }

    #[test]
    fn an_unclosed_brace_is_text() {
        assert_eq!(render("{unclosed", facts), "{unclosed");
        assert_eq!(render("{id} {unclosed", facts), "42 {unclosed");
    }

    /// A resolved value is compositor- or calendar-supplied text, so a token inside one must stay
    /// text rather than become a second round of substitution.
    #[test]
    fn a_token_inside_a_resolved_value_is_not_substituted() {
        assert_eq!(render("{name}", facts), "{id}");
    }

    #[test]
    fn a_longer_token_is_not_eaten_by_a_shorter_one_it_contains() {
        let resolve = |token: &str| match token {
            "name" => Some("short"),
            "name-or-index" => Some("long"),
            _ => None,
        };

        assert_eq!(render("{name-or-index}", resolve), "long");
        assert_eq!(render("{name}", resolve), "short");
    }
}
