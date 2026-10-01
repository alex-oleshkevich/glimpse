use super::*;

#[test]
fn an_absent_field_is_not_an_empty_one() {
    assert_eq!(none_if_empty(""), None);
    assert_eq!(none_if_empty("Marta Kaz"), Some("Marta Kaz"));
}
