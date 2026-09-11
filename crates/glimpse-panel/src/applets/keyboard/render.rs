use glimpse_contracts::{KeyboardLayout, KeyboardLayouts, LayoutRef};
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
        Some(format) => format
            .replace("{code}", &layout.code)
            .replace("{name}", &layout.name),
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
            code: layout.code.clone(),
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
mod tests {
    use super::*;

    fn layouts(current: Option<u8>) -> KeyboardLayouts {
        KeyboardLayouts {
            layouts: vec![
                KeyboardLayout {
                    code: "US".to_owned(),
                    name: "English (US)".to_owned(),
                },
                KeyboardLayout {
                    code: "LT".to_owned(),
                    name: "Lithuanian".to_owned(),
                },
                KeyboardLayout {
                    code: "RU".to_owned(),
                    name: "Russian".to_owned(),
                },
            ],
            current,
        }
    }

    #[test]
    fn hidden_below_two_layouts() {
        assert!(!shown(None));
        assert!(!shown(Some(&KeyboardLayouts {
            layouts: vec![KeyboardLayout {
                code: "US".to_owned(),
                name: "English (US)".to_owned(),
            }],
            current: Some(0),
        })));
        assert!(shown(Some(&layouts(Some(0)))));
    }

    #[test]
    fn badge_is_the_current_code() {
        assert_eq!(badge(&layouts(Some(2))), Some("RU"));
        assert_eq!(badge(&layouts(None)), Some("US"));
    }

    #[test]
    fn tooltip_defaults_to_the_name() {
        assert_eq!(
            tooltip(&layouts(Some(0)), None).as_deref(),
            Some("English (US)")
        );
        assert_eq!(
            tooltip(&layouts(Some(0)), Some("{code} · {name}")).as_deref(),
            Some("US · English (US)")
        );
    }

    #[test]
    fn rows_flag_the_current_layout() {
        let rows = rows(&layouts(Some(1)));
        assert_eq!(rows.len(), 3);
        assert!(!rows[0].active && rows[1].active && !rows[2].active);
        assert_eq!(rows[1].code, "LT");
    }

    #[test]
    fn next_and_prev_wrap() {
        assert_eq!(step(&layouts(Some(2)), LayoutRef::Next), Some(0));
        assert_eq!(step(&layouts(Some(0)), LayoutRef::Prev), Some(2));
    }
}
