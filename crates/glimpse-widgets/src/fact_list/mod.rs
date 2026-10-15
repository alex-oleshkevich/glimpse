use gtk4::prelude::*;

use crate::{Row, RowTone};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Fact {
    pub label: String,
    pub value: String,
    pub warning: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Line {
    pub action: String,
    pub title: String,
    pub toggle: Option<bool>,
    pub activates: bool,
    pub busy: bool,
    pub destructive: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Details {
    pub id: String,
    pub facts: Vec<Fact>,
    pub lines: Vec<Line>,
}

impl Fact {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            warning: false,
        }
    }

    pub fn warning(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            warning: true,
            ..Self::new(label, value)
        }
    }
}

pub fn set_facts(container: &gtk4::Box, facts: &[Fact]) {
    let mut rows = Vec::new();
    let mut child = container.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Ok(row) = widget.downcast::<Row>() {
            rows.push(row);
        }
    }
    for (index, fact) in facts.iter().enumerate() {
        let row = rows.get(index).cloned().unwrap_or_else(|| {
            let row = Row::new();
            container.append(&row);
            row
        });
        row.set_title(Some(fact.label.as_str()));
        row.set_value(Some(fact.value.as_str()));
        row.set_tone(match fact.warning {
            true => RowTone::Warning,
            false => RowTone::Normal,
        });
    }
    for row in rows.iter().skip(facts.len()) {
        container.remove(row);
    }
}
