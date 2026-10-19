use super::*;

#[test]
fn a_watch_on_coordinates_outside_their_ranges_is_refused() {
    for (latitude, longitude) in [(91.0, 0.0), (-91.0, 0.0), (0.0, 181.0), (0.0, -181.0)] {
        let refused = placed(at(latitude, longitude)).expect_err("not on Earth");

        assert!(matches!(refused, CommandError::InvalidArgument(_)));
    }

    for (latitude, longitude) in [(90.0, 180.0), (-90.0, -180.0), (0.0, 0.0)] {
        placed(at(latitude, longitude)).expect("the edges are on Earth");
    }
}

#[tokio::test]
async fn a_named_watch_is_normalized_and_requested_without_panel_coordinates() {
    let mut harness = harness().await;

    call(
        &mut harness,
        Asked::Watch(WatchedPlace::Location {
            name: "Vilnius, lt".to_owned(),
        }),
    )
    .await
    .expect("a named place");

    assert_eq!(
        harness.service.request().targets,
        vec![Target {
            place: WatchedPlace::Location {
                name: "Vilnius, LT".to_owned(),
            },
            coordinates: None,
            resolved: None,
        }]
    );
}

#[test]
fn a_named_watch_needs_a_city_and_two_letter_country_code() {
    for name in ["Vilnius", ", LT", "Vilnius, Lithuania"] {
        let refused = placed(WatchedPlace::Location {
            name: name.to_owned(),
        })
        .expect_err("not a City, CC location");

        assert!(matches!(refused, CommandError::InvalidArgument(_)));
    }
}

#[tokio::test]
async fn resolved_place_metadata_is_owned_by_the_weather_result() {
    let mut harness = harness().await;
    call(
        &mut harness,
        Asked::Watch(WatchedPlace::Location {
            name: "Wilno, LT".to_owned(),
        }),
    )
    .await
    .expect("a named place");

    harness.service.absorb(&Fetch {
        targets: vec![ResolvedTarget {
            place: WatchedPlace::Location {
                name: "Wilno, LT".to_owned(),
            },
            resolved: ResolvedPlace {
                coordinates: fix(54.6872, 25.2797),
                city: "Vilnius".to_owned(),
                country_code: "LT".to_owned(),
            },
        }],
        readings: vec![a_reading()],
        failure: None,
    });

    assert_eq!(harness.service.places[0].city.as_deref(), Some("Vilnius"));
    assert_eq!(
        harness.service.places[0].country_code.as_deref(),
        Some("LT")
    );
    assert_eq!(
        harness.service.places[0].place,
        WatchedPlace::Location {
            name: "Wilno, LT".to_owned(),
        }
    );
}
