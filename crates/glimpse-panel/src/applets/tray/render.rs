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
mod tests {
    use super::*;

    fn item(id: &str, status: TrayStatus) -> TrayItem {
        TrayItem {
            key: format!(":1.1/{id}"),
            id: id.to_owned(),
            status,
            ..Default::default()
        }
    }

    fn ids(items: &[&TrayItem]) -> Vec<String> {
        items.iter().map(|item| item.id.clone()).collect()
    }

    #[test]
    fn an_empty_tray_arranges_to_nothing() {
        assert!(arrange(&[], &[], &[]).is_empty());
    }

    #[test]
    fn registration_order_is_kept_when_nothing_is_pinned_or_hidden() {
        let items = [
            item("first", TrayStatus::Active),
            item("second", TrayStatus::Active),
        ];
        assert_eq!(ids(&arrange(&items, &[], &[])), ["first", "second"]);
    }

    #[test]
    fn a_hidden_id_is_dropped_and_the_rest_close_up() {
        let items = [
            item("keep", TrayStatus::Active),
            item("drop", TrayStatus::Active),
            item("also", TrayStatus::Active),
        ];
        assert_eq!(
            ids(&arrange(&items, &["drop".to_owned()], &[])),
            ["keep", "also"]
        );
    }

    #[test]
    fn pinned_items_lead_in_the_order_they_were_named() {
        let items = [
            item("a", TrayStatus::Active),
            item("b", TrayStatus::Active),
            item("c", TrayStatus::Active),
        ];
        assert_eq!(
            ids(&arrange(&items, &[], &["c".to_owned(), "a".to_owned()])),
            ["c", "a", "b"],
            "the pin order is the user's, not the registry's"
        );
    }

    #[test]
    fn attention_leads_and_passive_sorts_toward_the_chevron() {
        let items = [
            item("calm", TrayStatus::Active),
            item("away", TrayStatus::Passive),
            item("loud", TrayStatus::NeedsAttention),
        ];
        assert_eq!(
            ids(&arrange(&items, &[], &[])),
            ["loud", "calm", "away"],
            "a cap of two then leaves exactly the passive one behind the chevron"
        );
    }

    #[test]
    fn a_pin_outranks_the_status_order() {
        let items = [
            item("quiet", TrayStatus::Passive),
            item("loud", TrayStatus::NeedsAttention),
        ];
        assert_eq!(
            ids(&arrange(&items, &[], &["quiet".to_owned()])),
            ["quiet", "loud"]
        );
    }

    #[test]
    fn a_hidden_id_stays_hidden_even_when_it_is_also_pinned() {
        let items = [item("both", TrayStatus::Active)];
        assert!(
            arrange(&items, &["both".to_owned()], &["both".to_owned()]).is_empty(),
            "hiding is the stronger statement; a contradiction must not show the item twice"
        );
    }

    #[test]
    fn the_same_id_pinned_twice_still_shows_one_chip() {
        let items = [item("a", TrayStatus::Active), item("b", TrayStatus::Active)];
        assert_eq!(
            ids(&arrange(&items, &[], &["a".to_owned(), "a".to_owned()])),
            ["a", "b"],
            "two chips sharing a key make reconcile build a second widget for one item"
        );
    }

    #[test]
    fn a_cap_below_the_pinned_count_is_raised_to_keep_the_promise() {
        let items = [
            item("a", TrayStatus::Active),
            item("b", TrayStatus::Active),
            item("c", TrayStatus::Active),
            item("d", TrayStatus::Active),
        ];
        let pin = ["a".to_owned(), "b".to_owned(), "c".to_owned()];
        let shown = arrange(&items, &[], &pin);

        assert_eq!(
            cap(&shown, &pin, 2),
            3,
            "`pin` promises the bar whatever the cap says"
        );
        assert_eq!(
            hidden(shown.len(), cap(&shown, &pin, 2)),
            1,
            "only the unpinned one hides"
        );
        assert_eq!(cap(&shown, &pin, 4), 4, "a roomier cap is left alone");
        assert_eq!(cap(&shown, &pin, 0), 0, "zero already means no overflow");
        assert_eq!(
            cap(&shown, &["absent".to_owned()], 2),
            2,
            "a pinned id no application is offering reserves nothing"
        );
    }

    #[test]
    fn a_cap_of_zero_hides_nothing_rather_than_everything() {
        assert_eq!(hidden(5, 0), 0);
        assert_eq!(hidden(5, 9), 0);
        assert_eq!(hidden(5, 5), 0);
        assert_eq!(hidden(5, 2), 3);
        assert_eq!(hidden(0, 2), 0);
    }

    #[test]
    fn a_tooltip_prefers_the_applications_own_and_joins_its_two_halves() {
        let mut shown = item("x", TrayStatus::Active);
        shown.title = "Nextcloud".to_owned();
        assert_eq!(tooltip(&shown).as_deref(), Some("Nextcloud"));

        shown.tooltip = Some(glimpse_dbus::status_notifier_item::TrayTooltip {
            title: "Nextcloud".to_owned(),
            body: "Syncing 12 of 340 files".to_owned(),
            ..Default::default()
        });
        assert_eq!(
            tooltip(&shown).as_deref(),
            Some("Nextcloud\nSyncing 12 of 340 files")
        );

        shown.tooltip = Some(glimpse_dbus::status_notifier_item::TrayTooltip {
            title: "You have 1 notification".to_owned(),
            ..Default::default()
        });
        assert_eq!(
            tooltip(&shown).as_deref(),
            Some("You have 1 notification"),
            "an application that puts prose in the title slot still gets one line"
        );
    }

    #[test]
    fn an_item_with_no_text_at_all_gets_no_tooltip_rather_than_an_empty_one() {
        assert_eq!(tooltip(&item("x", TrayStatus::Active)), None);
    }
}
