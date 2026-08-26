use serde_json::Value;

/// One function per colour. Each returns the text wrapped in its escapes, or an empty string for
/// empty text: escapes around nothing still sit at the end of a line, where they stop anything
/// from trimming the padding in front of them, and a caller can lean on that to make an optional
/// marker disappear.
pub mod styled {
    use anstyle::{AnsiColor, Color, Style};

    pub fn header(text: &str) -> String {
        apply(&text.to_uppercase(), Style::new())
    }

    pub fn key(text: &str) -> String {
        apply(text, Style::new().dimmed())
    }

    pub fn good(text: &str) -> String {
        apply(text, ansi(AnsiColor::Green))
    }

    pub fn warn(text: &str) -> String {
        apply(text, ansi(AnsiColor::Yellow))
    }

    pub fn bad(text: &str) -> String {
        apply(text, ansi(AnsiColor::Red))
    }

    const fn ansi(color: AnsiColor) -> Style {
        Style::new().fg_color(Some(Color::Ansi(color)))
    }

    fn apply(text: &str, style: Style) -> String {
        match text.is_empty() {
            true => String::new(),
            false => format!("{}{text}{}", style.render(), style.render_reset()),
        }
    }
}

pub struct Table<const N: usize> {
    headers: Option<[String; N]>,
    rows: Vec<[String; N]>,
    empty: Option<String>,
}

impl<const N: usize> Default for Table<N> {
    fn default() -> Self {
        Self {
            headers: None,
            rows: Vec::new(),
            empty: None,
        }
    }
}

impl<const N: usize> Table<N> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_headers(mut self, headers: [&str; N]) -> Self {
        self.headers = Some(headers.map(styled::header));
        self
    }

    /// What to say instead when there are no rows. Headers over nothing look like a table that
    /// failed rather than a question with no answer.
    pub fn with_empty(mut self, message: &str) -> Self {
        self.empty = Some(styled::key(message));
        self
    }

    pub fn with_rows(self, rows: impl IntoIterator<Item = [String; N]>) -> Self {
        rows.into_iter().fold(self, Self::with_row)
    }

    pub fn with_row(mut self, row: [String; N]) -> Self {
        self.rows.push(row);
        self
    }

    pub fn render(self) -> String {
        if let (true, Some(empty)) = (self.rows.is_empty(), &self.empty) {
            return empty.clone();
        }

        let rows: Vec<&[String; N]> = self.headers.iter().chain(self.rows.iter()).collect();
        let widths: [usize; N] = std::array::from_fn(|column| {
            rows.iter()
                .map(|row| width(&row[column]))
                .max()
                .unwrap_or(0)
        });

        rows.iter()
            .map(|row| {
                row.iter()
                    .enumerate()
                    .map(|(column, cell)| match column == N - 1 {
                        true => cell.clone(),
                        false => cell.clone() + &" ".repeat(widths[column] - width(cell)),
                    })
                    .collect::<Vec<_>>()
                    .join("  ")
                    // An empty last cell still gets a separator in front of it, and `styled` leaves
                    // nothing behind for an empty string, so there is nothing between here and it.
                    .trim_end()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Columns the text occupies once the terminal has eaten the escapes.
///
/// Counts characters, so a wide glyph — CJK, an emoji — is measured one column short and its row
/// hangs by the difference. Fixing that properly needs `unicode-width`; nothing published today
/// puts a wide glyph in a padded column.
fn width(text: &str) -> usize {
    visible(text).chars().count()
}

fn visible(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(char) = chars.next() {
        if char == '\u{1b}' {
            for escape in chars.by_ref() {
                if escape == 'm' {
                    break;
                }
            }
        } else {
            out.push(char);
        }
    }
    out
}

/// A titled block: a heading, indented content, and an optional note saying what the content means.
///
/// Content arrives already rendered, as a `String`, so a section composes with a `Table`, with
/// `lines`, with another section or with a bare sentence, and never needs to know which.
pub struct Section {
    title: String,
    body: Vec<String>,
    note: Option<String>,
}

impl Section {
    pub fn new(title: &str) -> Self {
        Self {
            title: styled::header(title),
            body: Vec::new(),
            note: None,
        }
    }

    pub fn with(mut self, content: impl Into<String>) -> Self {
        self.body.push(content.into());
        self
    }

    /// What the reader should take from the block above — the consequence, not a repeat of it.
    pub fn with_note(mut self, note: &str) -> Self {
        self.note = Some(styled::key(&format!("→ {note}")));
        self
    }

    pub fn render(self) -> String {
        let body = self.body.iter().map(String::as_str);
        let note = self.note.iter().map(String::as_str);

        std::iter::once(self.title.as_str())
            .chain(
                body.chain(note)
                    .map(indent)
                    .collect::<Vec<_>>()
                    .iter()
                    .map(String::as_str),
            )
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Blank line between blocks, which is what separates one section from the next.
pub fn stacked(blocks: impl IntoIterator<Item = String>) -> String {
    blocks.into_iter().collect::<Vec<_>>().join("\n\n")
}

/// An empty line stays empty: indenting it would leave whitespace nothing can see.
fn indent(block: &str) -> String {
    block
        .lines()
        .map(|line| match line.is_empty() {
            true => String::new(),
            false => format!("  {line}"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A payload as one leaf per line, `path  value`, aligned. A payload that is a bare scalar prints
/// as itself, which is what makes `get --field` usable from a script.
pub fn lines(value: &Value) -> String {
    let leaves = leaves(value);
    if let [(path, only)] = leaves.as_slice()
        && path.is_empty()
    {
        return only.clone();
    }

    Table::new()
        .with_rows(
            leaves
                .iter()
                .map(|(path, leaf)| [styled::key(path), leaf.clone()]),
        )
        .render()
}

/// A payload as one line of `path=value`, so a stream of them stays greppable.
pub fn inline(value: &Value) -> String {
    leaves(value)
        .iter()
        .map(|(path, leaf)| match path.is_empty() {
            true => leaf.clone(),
            false => format!("{path}={leaf}"),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Every scalar in a payload, as `dotted.path` and text. Objects nest with `.`, arrays with `[n]`,
/// and an empty collection is a leaf so it is never silently dropped. A bare scalar is one leaf
/// with an empty path.
fn leaves(value: &Value) -> Vec<(String, String)> {
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
mod tests {
    use super::*;

    fn row<const N: usize>(texts: [&str; N]) -> [String; N] {
        texts.map(str::to_owned)
    }

    #[test]
    fn columns_align_to_their_widest_cell() {
        let rendered = Table::new()
            .with_headers(["NAME", "STATE"])
            .with_rows([row(["audio", "running"])])
            .with_row(row(["network", "degraded"]))
            .render();
        assert_eq!(
            visible(&rendered),
            "NAME     STATE\naudio    running\nnetwork  degraded"
        );
    }

    #[test]
    fn a_styled_cell_does_not_shift_its_column() {
        let rendered = Table::new()
            .with_rows([
                [styled::good("audio"), "running".to_owned()],
                [styled::bad("network"), "degraded".to_owned()],
            ])
            .render();
        assert_eq!(visible(&rendered), "audio    running\nnetwork  degraded");
    }

    #[test]
    fn an_empty_trailing_cell_leaves_no_whitespace() {
        let rendered = Table::new()
            .with_row(row(["audio", "running", ""]))
            .render();
        assert_eq!(visible(&rendered), "audio  running");
        assert!(
            !rendered.ends_with(' '),
            "an empty last cell left its separator behind"
        );
    }

    #[test]
    fn a_bare_scalar_prints_as_itself() {
        assert_eq!(lines(&serde_json::json!(42)), "42");
        assert_eq!(lines(&serde_json::json!("auto")), "auto");
    }

    #[test]
    fn a_payload_prints_one_leaf_per_line() {
        let data = serde_json::json!({ "at": { "lat": 52.2 }, "names": ["a", "b"] });
        assert_eq!(
            visible(&lines(&data)),
            "at.lat    52.2\nnames[0]  a\nnames[1]  b"
        );
    }

    #[test]
    fn a_payload_inlines_as_greppable_pairs() {
        let data = serde_json::json!({ "at": { "lat": 52.2 }, "phase": "day" });
        assert_eq!(inline(&data), "at.lat=52.2 phase=day");
    }

    #[test]
    fn no_rows_says_so_instead_of_printing_headers() {
        let rendered = Table::<2>::new()
            .with_headers(["NAME", "STATE"])
            .with_empty("nothing matches")
            .render();
        assert_eq!(visible(&rendered), "nothing matches");
    }

    #[test]
    fn rows_win_over_the_empty_message() {
        let rendered = Table::new()
            .with_headers(["NAME", "STATE"])
            .with_empty("nothing matches")
            .with_row(row(["audio", "running"]))
            .render();
        assert_eq!(visible(&rendered), "NAME   STATE\naudio  running");
    }

    #[test]
    fn a_section_indents_its_content_under_a_heading() {
        let rendered = Section::new("D-Bus")
            .with("session bus  ok")
            .with_note("network is degraded")
            .render();
        assert_eq!(
            visible(&rendered),
            "D-BUS\n  session bus  ok\n  → network is degraded"
        );
    }

    #[test]
    fn stacked_sections_are_separated_by_a_blank_line() {
        let rendered = stacked([
            Section::new("One").with("a").render(),
            Section::new("Two").with("b").render(),
        ]);
        assert_eq!(visible(&rendered), "ONE\n  a\n\nTWO\n  b");
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
