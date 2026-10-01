use super::*;

fn key(key: &str) -> Notification {
    Notification {
        key: key.to_owned(),
        ..Notification::default()
    }
}

#[test]
fn a_row_missing_from_the_target_list_and_not_already_fading_starts_removal() {
    let rows = ["a".to_owned(), "b".to_owned(), "c".to_owned()];
    let keep = [key("b")];
    let fading = ["c".to_owned()];
    assert_eq!(removal_keys(&rows, &keep, &fading), ["a".to_owned()]);
}

#[test]
fn a_key_already_fading_is_not_queued_a_second_time() {
    let rows = ["a".to_owned()];
    assert_eq!(
        removal_keys(&rows, &[], &["a".to_owned()]),
        Vec::<String>::new()
    );
}

#[test]
fn clearing_every_notification_queues_every_row() {
    let rows = ["a".to_owned(), "b".to_owned()];
    assert_eq!(removal_keys(&rows, &[], &[]), rows);
}
