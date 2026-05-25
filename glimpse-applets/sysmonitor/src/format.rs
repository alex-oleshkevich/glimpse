//! Token interpolation for indicator label/tooltip strings.
//!
//! Syntax: `{token}` for the bare value (`Display`), `{token:.N}` for
//! N-decimal floating-point formatting. An unknown token is left in the
//! output verbatim (including its braces) — that's louder than silent
//! omission when a user typoes a key, and Pango will simply render `{foo}`
//! as literal text on the panel.
//!
//! The format spec is intentionally a strict subset of Rust's `{:...}`
//! mini-language: we resolve `:.N` (precision) at runtime because the
//! template is loaded from a config file, not the source code.

use std::collections::HashMap;

/// Concrete value backing a token. Stays small and `Clone` so each tick's
/// values table is cheap to build.
#[derive(Debug, Clone)]
pub enum FormatValue {
    Float(f64),
    Int(i64),
    UInt(u64),
    Str(String),
}

impl From<f64> for FormatValue {
    fn from(v: f64) -> Self {
        Self::Float(v)
    }
}

impl From<u64> for FormatValue {
    fn from(v: u64) -> Self {
        Self::UInt(v)
    }
}

impl From<usize> for FormatValue {
    fn from(v: usize) -> Self {
        Self::UInt(v as u64)
    }
}

impl From<i64> for FormatValue {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}

impl From<&str> for FormatValue {
    fn from(v: &str) -> Self {
        Self::Str(v.to_owned())
    }
}

impl From<String> for FormatValue {
    fn from(v: String) -> Self {
        Self::Str(v)
    }
}

/// Renders `template` by substituting `{token}` and `{token:spec}`
/// placeholders against `values`. Unknown tokens are preserved as literal
/// text. Lone `{` characters (no matching `}`) are also preserved.
pub fn render(template: &str, values: &HashMap<&str, FormatValue>) -> String {
    let mut out = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            // Look for the closing brace.
            if let Some(rel_end) = template[i + 1..].find('}') {
                let end = i + 1 + rel_end;
                let placeholder = &template[i + 1..end];
                let (token, spec) = match placeholder.split_once(':') {
                    Some((t, s)) => (t.trim(), Some(s.trim())),
                    None => (placeholder.trim(), None),
                };
                match values.get(token) {
                    Some(value) => out.push_str(&format_value(value, spec)),
                    None => out.push_str(&template[i..=end]),
                }
                i = end + 1;
                continue;
            }
        }
        // Plain byte. Walk char-by-char so UTF-8 sequences aren't sliced.
        let ch = template[i..].chars().next().expect("non-empty remainder");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn format_value(value: &FormatValue, spec: Option<&str>) -> String {
    let precision = spec.and_then(parse_precision);
    match value {
        FormatValue::Float(f) => match precision {
            Some(p) => format!("{f:.p$}"),
            None => format!("{f}"),
        },
        FormatValue::Int(i) => i.to_string(),
        FormatValue::UInt(u) => u.to_string(),
        FormatValue::Str(s) => s.clone(),
    }
}

/// Parses `.N` (e.g. `".0"`, `".2"`) into the precision integer. Anything
/// else returns `None` (unrecognized spec → fall back to default Display).
/// Deliberately strict — we'd rather reject `:N` than guess.
fn parse_precision(spec: &str) -> Option<usize> {
    spec.strip_prefix('.').and_then(|rest| rest.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make<const N: usize>(pairs: [(&'static str, FormatValue); N]) -> HashMap<&'static str, FormatValue> {
        pairs.into_iter().collect()
    }

    #[test]
    fn substitutes_bare_token() {
        let out = render("CPU {pct}", &make([("pct", 12.0.into())]));
        assert_eq!(out, "CPU 12");
    }

    /// The headline use case: percent label without decimals.
    #[test]
    fn formats_float_with_precision() {
        let out = render(
            "{cpu_util_pct:.0}%",
            &make([("cpu_util_pct", FormatValue::Float(45.83))]),
        );
        assert_eq!(out, "46%");
    }

    /// `:.2` → two decimals, rounded-half-to-even by Rust's default.
    #[test]
    fn formats_float_with_two_decimals() {
        let out = render("{f:.2}", &make([("f", FormatValue::Float(3.4))]));
        assert_eq!(out, "3.40");
    }

    /// Integers ignore precision specs — `{cores:.0}` is the same as
    /// `{cores}` because precision is meaningless for integer Display.
    #[test]
    fn precision_on_int_is_ignored() {
        let out = render("{n:.2}", &make([("n", FormatValue::UInt(8))]));
        assert_eq!(out, "8");
    }

    /// Unknown token preserved verbatim. The user can grep their template
    /// and the literal `{foo}` is the smoking gun.
    #[test]
    fn unknown_token_left_untouched() {
        let out = render(
            "CPU {cpu_util_pct:.0}% {foo}",
            &make([("cpu_util_pct", FormatValue::Float(50.0))]),
        );
        assert_eq!(out, "CPU 50% {foo}");
    }

    /// Trailing `{` with no closing brace: keep as literal, don't loop or panic.
    #[test]
    fn unclosed_brace_is_literal() {
        let out = render("oops {cpu", &make([]));
        assert_eq!(out, "oops {cpu");
    }

    /// Multi-byte glyphs in the template (em-dash, emoji) and in token
    /// values must round-trip — the iterator walks chars, not bytes.
    #[test]
    fn utf8_in_template_and_values_round_trips() {
        let out = render(
            "★ {name} \u{2014} {pct:.0}%",
            &make([
                ("name", FormatValue::Str("CPU 🎉".into())),
                ("pct", FormatValue::Float(99.0)),
            ]),
        );
        assert_eq!(out, "★ CPU 🎉 \u{2014} 99%");
    }

    /// Whitespace inside placeholder is tolerated (`{ token :.0 }`) — TOML
    /// hand-formatting habits vary, and this is forgiving without being lossy.
    #[test]
    fn whitespace_around_token_and_spec_is_trimmed() {
        let out = render("{ pct :.0}%", &make([("pct", FormatValue::Float(50.0))]));
        assert_eq!(out, "50%");
    }

    /// Empty template → empty output; no panic on the zero case.
    #[test]
    fn empty_template_is_empty() {
        assert_eq!(render("", &make([])), "");
    }
}
