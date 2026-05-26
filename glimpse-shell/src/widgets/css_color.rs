pub fn sanitize_css_color(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return None;
    }
    if trimmed.bytes().any(|b| matches!(b, b';' | b'{' | b'}')) {
        return None;
    }
    if is_hex_color(trimmed) || is_color_function(trimmed) || is_named_color(trimmed) {
        return Some(trimmed.to_string());
    }
    None
}

fn is_hex_color(value: &str) -> bool {
    let Some(rest) = value.strip_prefix('#') else {
        return false;
    };
    matches!(rest.len(), 3 | 4 | 6 | 8) && rest.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_color_function(value: &str) -> bool {
    const FUNCS: &[&str] = &[
        "rgb(", "rgba(", "hsl(", "hsla(", "hwb(", "lab(", "lch(", "oklab(", "oklch(", "color(",
    ];
    let lower = value.to_ascii_lowercase();
    FUNCS.iter().any(|f| lower.starts_with(f)) && lower.ends_with(')')
}

fn is_named_color(value: &str) -> bool {
    value
        .bytes()
        .all(|b| b.is_ascii_alphabetic() || b == b'-' || b == b'_')
        && !value.is_empty()
}

#[cfg(test)]
mod tests {
    use super::sanitize_css_color;

    #[test]
    fn accepts_hex_colors() {
        assert_eq!(sanitize_css_color("#abc").as_deref(), Some("#abc"));
        assert_eq!(sanitize_css_color("#aabbcc").as_deref(), Some("#aabbcc"));
        assert_eq!(
            sanitize_css_color("#aabbccdd").as_deref(),
            Some("#aabbccdd")
        );
    }

    #[test]
    fn accepts_color_functions() {
        assert!(sanitize_css_color("rgb(1,2,3)").is_some());
        assert!(sanitize_css_color("oklch(0.5 0.1 200)").is_some());
        assert!(sanitize_css_color("HSL(0, 0%, 0%)").is_some());
    }

    #[test]
    fn rejects_injection() {
        assert!(sanitize_css_color("red; background: blue").is_none());
        assert!(sanitize_css_color("} html { display:none").is_none());
        assert!(sanitize_css_color("#xyz").is_none());
        assert!(sanitize_css_color("").is_none());
    }
}
