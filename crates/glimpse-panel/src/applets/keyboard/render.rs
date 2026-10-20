use glimpse_services::{KeyboardLayout, KeyboardLayouts, LayoutRef};
use glimpse_widgets::KeyboardLayout as Row;

pub fn shown(layouts: Option<&KeyboardLayouts>) -> bool {
    layouts.is_some_and(|layouts| layouts.layouts.len() >= 2)
}

pub fn current(layouts: &KeyboardLayouts) -> Option<&KeyboardLayout> {
    if !shown(Some(layouts)) {
        return None;
    }
    layouts
        .current
        .and_then(|index| layouts.layouts.get(index as usize))
        .or_else(|| layouts.layouts.first())
}

pub fn badge(layouts: &KeyboardLayouts) -> Option<&str> {
    current(layouts).map(|layout| layout.code.as_str())
}

pub fn tooltip(layouts: &KeyboardLayouts, format: Option<&str>) -> Option<String> {
    let layout = current(layouts)?;
    Some(match format {
        Some(format) => crate::applets::tokens::render(format, |token| match token {
            "code" => Some(layout.code.as_str()),
            "name" => Some(layout.name.as_str()),
            _ => None,
        }),
        None => layout.name.clone(),
    })
}

pub fn rows(layouts: &KeyboardLayouts) -> Vec<Row> {
    layouts
        .layouts
        .iter()
        .enumerate()
        .map(|(index, layout)| Row {
            name: layout.name.clone(),
            active: layouts.current == Some(index as u8),
        })
        .collect()
}

pub fn step(layouts: &KeyboardLayouts, target: LayoutRef) -> Option<u8> {
    let n = u8::try_from(layouts.layouts.len())
        .ok()
        .filter(|&n| n > 0)?;
    let current = layouts.current.unwrap_or(0);
    Some(match target {
        LayoutRef::Next => (current + 1) % n,
        LayoutRef::Prev => current.checked_sub(1).unwrap_or(n - 1),
        LayoutRef::Index { index } => index.min(n - 1),
    })
}

#[cfg(test)]
mod tests;
