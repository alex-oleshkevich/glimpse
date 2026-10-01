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
mod tests;
