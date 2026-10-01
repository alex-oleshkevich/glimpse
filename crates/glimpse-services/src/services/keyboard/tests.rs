use super::*;

fn backend(names: &[&str], codes: &[&str], current: Option<usize>) -> BackendLayouts {
    BackendLayouts {
        names: names.iter().map(|name| (*name).to_owned()).collect(),
        codes: codes.iter().map(|code| (*code).to_owned()).collect(),
        current,
    }
}

fn window(id: u64, app_id: &str, focused: bool) -> WindowInfo {
    WindowInfo {
        id,
        title: None,
        app_id: Some(app_id.to_owned()),
        workspace: None,
        focused,
        floating: false,
        urgent: false,
        order: None,
    }
}

#[test]
fn two_layouts_publish_uppercase_codes() {
    let published = payload(
        &backend(&["English (US)", "Russian"], &["us", "ru"], Some(1)),
        &BTreeMap::new(),
    );
    assert_eq!(
        published.layouts,
        [
            KeyboardLayout {
                code: "US".to_owned(),
                name: "English (US)".to_owned(),
            },
            KeyboardLayout {
                code: "RU".to_owned(),
                name: "Russian".to_owned(),
            },
        ]
    );
    assert_eq!(published.current, Some(1));
}

#[test]
fn a_label_overrides_the_badge() {
    let mut labels = BTreeMap::new();
    labels.insert("us".to_owned(), "EN".to_owned());
    let published = payload(&backend(&["English (US)"], &["us"], Some(0)), &labels);
    assert_eq!(published.layouts[0].code, "EN");
}

#[test]
fn an_empty_list_has_no_current() {
    let published = payload(&BackendLayouts::default(), &BTreeMap::new());
    assert!(published.layouts.is_empty());
    assert_eq!(published.current, None);
}

#[test]
fn a_current_past_the_end_is_dropped() {
    let published = payload(
        &backend(&["English (US)"], &["us"], Some(4)),
        &BTreeMap::new(),
    );
    assert_eq!(published.current, None);
}

#[test]
fn names_are_capped_by_characters() {
    let long = "я".repeat(NAME_CAP + 8);
    let published = payload(&backend(&[&long], &["ru"], Some(0)), &BTreeMap::new());
    assert_eq!(published.layouts[0].name.chars().count(), NAME_CAP);
}

#[test]
fn a_layout_switch_updates_the_current_index() {
    let mut state = backend(&["English (US)", "Russian"], &["us", "ru"], Some(0));
    assert!(matches!(
        apply_keyboard(
            &mut state,
            Change::KeyboardLayoutSwitched {
                idx: 1,
                name: Some("Russian".to_owned()),
            }
        ),
        Apply::Publish
    ));
    assert_eq!(state.current, Some(1));
}

#[test]
fn a_keyboard_resync_asks_for_a_new_snapshot() {
    let mut state = BackendLayouts::default();
    assert!(matches!(
        apply_keyboard(&mut state, Change::Resync(Resync::Keyboard)),
        Apply::Refetch
    ));
    assert!(matches!(
        apply_keyboard(&mut state, Change::Resync(Resync::Structure)),
        Apply::Ignore
    ));
}

#[test]
fn global_remember_has_no_key() {
    assert_eq!(
        remember_key(Remember::Global, Some(&window(1, "firefox", true))),
        None
    );
}

#[test]
fn window_remember_keys_on_the_focused_id() {
    assert_eq!(
        remember_key(Remember::Window, Some(&window(7, "firefox", true))),
        Some(StoreKey::Window(7))
    );
}

#[test]
fn app_remember_keys_on_the_app_id() {
    assert_eq!(
        remember_key(Remember::App, Some(&window(7, "firefox", true))),
        Some(StoreKey::App("firefox".to_owned()))
    );
    assert_eq!(
        remember_key(
            Remember::App,
            Some(&WindowInfo {
                app_id: None,
                ..window(7, "", true)
            })
        ),
        None
    );
}

#[test]
fn next_and_prev_wrap() {
    assert_eq!(stepped(1, LayoutRef::Next, 2), Some(0));
    assert_eq!(stepped(0, LayoutRef::Prev, 2), Some(1));
}
