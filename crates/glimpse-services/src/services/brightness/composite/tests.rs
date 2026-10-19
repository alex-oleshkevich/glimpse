use super::*;
use crate::services::brightness::tests::FakeBacklight;

fn entry(id: &str) -> Entry {
    Entry {
        id: id.to_owned(),
        device_link: None,
        type_name: String::new(),
        brightness: 1,
        max_brightness: 2,
        kind: crate::services::brightness::Kind::Display,
    }
}

#[tokio::test]
async fn enumerate_merges_both_backends() {
    let primary = FakeBacklight::new(vec![entry("panel")]);
    let ddc = FakeBacklight::new(vec![entry("ddc:5")]);
    let composite = CompositeBacklight::new(Arc::new(primary), Arc::new(ddc));

    let entries = composite.enumerate().await.entries;

    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|entry| entry.id == "panel"));
    assert!(entries.iter().any(|entry| entry.id == "ddc:5"));
}

#[tokio::test]
async fn a_write_is_routed_by_the_ddc_prefix() {
    let primary = FakeBacklight::new(vec![entry("panel")]);
    let ddc = FakeBacklight::new(vec![entry("ddc:5")]);
    let composite = CompositeBacklight::new(Arc::new(primary.clone()), Arc::new(ddc.clone()));

    composite
        .write("panel".to_owned(), 50)
        .await
        .expect("the primary backend accepts it");
    composite
        .write("ddc:5".to_owned(), 60)
        .await
        .expect("the ddc backend accepts it");

    assert_eq!(primary.writes(), vec![("panel".to_owned(), 50)]);
    assert_eq!(ddc.writes(), vec![("ddc:5".to_owned(), 60)]);
}
