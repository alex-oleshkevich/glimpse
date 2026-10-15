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

pub fn render_lines<'a>(template: &str, resolve: impl Fn(&str) -> Option<&'a str>) -> String {
    template
        .lines()
        .filter(|line| {
            let mut known = false;
            let mut filled = false;
            let mut rest = *line;
            while let Some(open) = rest.find('{') {
                let Some(close) = rest[open..].find('}') else {
                    break;
                };
                if let Some(value) = resolve(&rest[open + 1..open + close]) {
                    known = true;
                    filled |= !value.trim().is_empty();
                }
                rest = &rest[open + close + 1..];
            }
            !known || filled
        })
        .map(|line| render(line, &resolve))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests;
