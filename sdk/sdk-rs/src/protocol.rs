use serde_json::Value;

pub(crate) const MAX_LINE: usize = 1_048_576;
pub(crate) const MAX_NODES: usize = 300;
pub(crate) const MAX_CHILDREN: usize = 200;
pub(crate) const MAX_OPS: usize = 1200;
pub(crate) const MAX_ITEMS: usize = 64;
pub(crate) const MAX_SERIES: usize = 366;

pub(crate) const CONTROLLED: &[(&str, &str, &str)] = &[
    ("entry", "onChange", "value"),
    ("scale", "onChange", "value"),
    ("switch", "onToggle", "active"),
    ("togglerow", "onToggle", "active"),
    ("togglebutton", "onToggle", "active"),
    ("checkbutton", "onToggle", "active"),
    ("expanderrow", "onToggle", "open"),
    ("sliderrow", "onChange", "value"),
    ("dropdown", "onSelect", "selected"),
    ("choicelist", "onSelect", "selected"),
];

pub(crate) fn seq_of(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        value
            .as_f64()
            .filter(|seq| seq.is_finite() && *seq >= 0.0)
            .map(|seq| seq as u64)
    })
}

pub(crate) fn controlled(kind: &str) -> Option<(&'static str, &'static str)> {
    CONTROLLED
        .iter()
        .find(|(name, _, _)| *name == kind)
        .map(|(_, event, prop)| (*event, *prop))
}

#[derive(Debug)]
pub enum Fail {
    Limit(String),
    Render(String),
    Io(String),
    Unavailable(String),
}

impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Limit(message)
            | Self::Render(message)
            | Self::Io(message)
            | Self::Unavailable(message) => f.write_str(message),
        }
    }
}

pub type Check<T> = Result<T, Fail>;

pub(crate) fn cap(kind: &str, key: &str, value: &str) -> String {
    let limit = match key {
        "badge" => 8,
        "value" if kind == "entry" => 4096,
        _ => 256,
    };
    value.chars().take(limit).collect()
}

pub(crate) fn oversized(value: &Value, limit: usize) -> bool {
    match value {
        Value::Array(items) => {
            items.len() > limit || items.iter().any(|item| oversized(item, MAX_ITEMS))
        }
        Value::Object(fields) => fields.values().any(|item| oversized(item, MAX_ITEMS)),
        _ => false,
    }
}

pub enum In {
    Line(String),
    Send(String),
    Wake,
    Eof,
}
