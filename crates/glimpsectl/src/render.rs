//! Column alignment, styling and JSON flattening. Nothing here knows what a topic or a service is
//! — what goes in the columns belongs to the command that has the data.

use anstyle::{AnsiColor, Color, Style};
use serde_json::Value;

const fn ansi(color: AnsiColor) -> Style {
    Style::new().fg_color(Some(Color::Ansi(color)))
}

pub const HEADER: Style = Style::new();
pub const KEY: Style = Style::new().dimmed();
pub const PLAIN: Style = Style::new();
pub const GOOD: Style = ansi(AnsiColor::Green);
pub const WARN: Style = ansi(AnsiColor::Yellow);
pub const BAD: Style = ansi(AnsiColor::Red);

pub type Row<const N: usize> = [(String, Style); N];

pub fn cells<const N: usize>(texts: [String; N], styles: [Style; N]) -> Row<N> {
    std::array::from_fn(|column| (texts[column].clone(), styles[column]))
}

/// A header row over aligned rows.
pub fn table<const N: usize>(headers: [&str; N], body: impl Iterator<Item = Row<N>>) -> String {
    let headers: Row<N> = headers.map(|header| (header.to_owned(), HEADER));
    rows(std::iter::once(headers).chain(body))
}

/// Rows aligned to their widest cell. Widths are measured on the text alone and the escapes added
/// after, so styling never shifts a column.
pub fn rows<const N: usize>(rows: impl Iterator<Item = Row<N>>) -> String {
    let rows: Vec<Row<N>> = rows.collect();
    let widths: [usize; N] = std::array::from_fn(|column| {
        rows.iter()
            .map(|row| row[column].0.len())
            .max()
            .unwrap_or(0)
    });

    rows.iter()
        .map(|row| {
            row.iter()
                .enumerate()
                .map(|(column, (text, style))| match column == N - 1 {
                    true => styled(text, *style),
                    false => styled(text, *style) + &" ".repeat(widths[column] - text.len()),
                })
                .collect::<Vec<_>>()
                .join("  ")
                // An empty last cell still gets a separator in front of it. `styled` guarantees it
                // contributed no escapes, so there is nothing between here and that padding.
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Empty text contributes nothing at all — escapes wrapped around no text still sit at the end of
/// a line, where they stop anything from trimming the padding in front of them, and a caller can
/// lean on that to make an optional marker disappear.
pub fn styled(text: &str, style: Style) -> String {
    match text.is_empty() {
        true => String::new(),
        false => format!("{}{text}{}", style.render(), style.render_reset()),
    }
}

/// Every scalar in a payload, as `dotted.path` and text. Objects nest with `.`, arrays with `[n]`,
/// and an empty collection is a leaf so it is never silently dropped. A bare scalar is one leaf
/// with an empty path.
pub fn leaves(value: &Value) -> Vec<(String, String)> {
    let mut found = Vec::new();
    walk(String::new(), value, &mut found);
    found
}

fn walk(path: String, value: &Value, found: &mut Vec<(String, String)>) {
    match value {
        Value::Object(fields) if !fields.is_empty() => {
            for (key, field) in fields {
                let child = match path.is_empty() {
                    true => key.clone(),
                    false => format!("{path}.{key}"),
                };
                walk(child, field, found);
            }
        }
        Value::Array(items) if !items.is_empty() => {
            for (index, item) in items.iter().enumerate() {
                walk(format!("{path}[{index}]"), item, found);
            }
        }
        Value::String(text) => found.push((path, text.clone())),
        Value::Object(_) => found.push((path, "{}".to_owned())),
        Value::Array(_) => found.push((path, "[]".to_owned())),
        scalar => found.push((path, scalar.to_string())),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Assertions are on the text, not the escapes: that styling is applied at all is what the eye
    /// checks, and pinning byte sequences would only make the tests brittle.
    pub(crate) fn plain(styled: &str) -> String {
        let mut out = String::new();
        let mut chars = styled.chars();
        while let Some(char) = chars.next() {
            if char == '\u{1b}' {
                for skip in chars.by_ref() {
                    if skip == 'm' {
                        break;
                    }
                }
            } else {
                out.push(char);
            }
        }
        out
    }

    fn row<const N: usize>(texts: [&str; N]) -> Row<N> {
        texts.map(|text| (text.to_owned(), PLAIN))
    }

    #[test]
    fn columns_align_to_their_widest_cell() {
        let rendered = table(
            ["NAME", "STATE"],
            [row(["audio", "running"]), row(["network", "degraded"])].into_iter(),
        );
        assert_eq!(
            plain(&rendered),
            "NAME     STATE\naudio    running\nnetwork  degraded"
        );
    }

    #[test]
    fn an_empty_trailing_cell_leaves_no_whitespace() {
        let rendered = rows([row(["audio", "running", ""])].into_iter());
        assert_eq!(plain(&rendered), "audio  running");
        assert!(
            !rendered.ends_with(' '),
            "an empty last cell left its separator behind"
        );
    }

    #[test]
    fn leaves_flatten_to_dotted_paths() {
        let data = serde_json::json!({ "at": { "lat": 52.2 }, "names": ["a"], "none": [] });
        assert_eq!(
            leaves(&data),
            [
                ("at.lat".to_owned(), "52.2".to_owned()),
                ("names[0]".to_owned(), "a".to_owned()),
                ("none".to_owned(), "[]".to_owned()),
            ]
        );
    }

    #[test]
    fn a_bare_scalar_is_one_leaf_with_no_path() {
        assert_eq!(
            leaves(&serde_json::json!("auto")),
            [(String::new(), "auto".to_owned())]
        );
    }
}
