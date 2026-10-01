use super::*;

#[test]
fn the_discovery_filter_names_the_transport_and_nothing_else() {
    let filter = filter();

    assert_eq!(filter.len(), 1);
    assert_eq!(filter.get("Transport"), Some(&Value::from("auto")));
}
