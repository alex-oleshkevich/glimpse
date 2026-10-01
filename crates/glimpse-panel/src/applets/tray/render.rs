use glimpse_dbus::status_notifier_item::{TrayItem, TrayStatus};

/// Which items the bar shows and in what order. Pinned first in the order they were named, then
/// whatever is active, then the passive ones — `Passive` is the item saying "put me away", so it
/// sorts toward the chevron rather than being hidden outright.
///
/// `hide` and `pin` match the item's `Id`, not its key: the id survives an application restart and
/// the bus name it happens to hold does not.
pub fn arrange<'a>(items: &'a [TrayItem], hide: &[String], pin: &[String]) -> Vec<&'a TrayItem> {
    let shown: Vec<&TrayItem> = items
        .iter()
        .filter(|item| !hide.iter().any(|id| id == &item.id))
        .collect();

    let mut ordered: Vec<&TrayItem> = Vec::with_capacity(shown.len());
    for id in pin {
        // A key may appear once. `pin` is hand-written, so the same id twice is an easy mistake,
        // and two chips sharing a key make `reconcile::by_key` build a second widget for it.
        for item in shown.iter().copied().filter(|item| &item.id == id) {
            if !ordered.iter().any(|held| held.key == item.key) {
                ordered.push(item);
            }
        }
    }
    let pinned = |item: &TrayItem| pin.iter().any(|id| id == &item.id);
    for status in [
        TrayStatus::NeedsAttention,
        TrayStatus::Active,
        TrayStatus::Passive,
    ] {
        ordered.extend(
            shown
                .iter()
                .copied()
                .filter(|item| !pinned(item) && item.status == status),
        );
    }
    ordered
}

/// The cap actually applied. `pin` is documented as keeping an item on the bar whatever the cap,
/// so a cap below the number of pinned items present is raised to fit them; `0` stays `0`, because
/// it already means no overflow at all.
pub fn cap(shown: &[&TrayItem], pin: &[String], max_visible: u8) -> u8 {
    if max_visible == 0 {
        return 0;
    }
    let pinned = shown
        .iter()
        .filter(|item| pin.iter().any(|id| id == &item.id))
        .count();
    max_visible.max(u8::try_from(pinned).unwrap_or(u8::MAX))
}

/// How many chips sit behind the chevron. `0` is the documented "no overflow at all", so it must
/// not read as "hide everything" — the two differ only here, and getting it wrong empties the bar.
pub fn hidden(shown: usize, max_visible: u8) -> usize {
    match max_visible {
        0 => 0,
        max => shown.saturating_sub(usize::from(max)),
    }
}

/// What a chip says when the pointer rests on it. An application's own `ToolTip` wins; its title is
/// the fallback, and an item with neither gets none rather than an empty box.
pub fn tooltip(item: &TrayItem) -> Option<String> {
    if let Some(tooltip) = &item.tooltip {
        let joined = match (tooltip.title.as_str(), tooltip.body.as_str()) {
            ("", "") => String::new(),
            (title, "") => title.to_owned(),
            ("", body) => body.to_owned(),
            (title, body) => format!("{title}\n{body}"),
        };
        if !joined.is_empty() {
            return Some(joined);
        }
    }
    Some(item.title.clone()).filter(|title| !title.is_empty())
}

#[cfg(test)]
mod tests;
