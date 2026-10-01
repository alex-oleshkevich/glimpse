use super::*;

#[test]
fn a_named_location_is_split_and_normalized() {
    assert_eq!(
        named("Vilnius, lt"),
        Ok(("Vilnius".to_owned(), "LT".to_owned()))
    );
    assert!(named("Vilnius").is_err());
    assert!(named("Vilnius, Lithuania").is_err());
}

#[tokio::test]
async fn coordinates_resolve_without_a_network_service() {
    let place = tokio::task::spawn_blocking(|| {
        reverse(GeoCoordinates {
            latitude: 52.2297,
            longitude: 21.0122,
        })
    })
    .await
    .unwrap();

    assert_eq!(place.city, "Warsaw");
    assert_eq!(place.country_code, "PL");
}
