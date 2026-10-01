use super::*;

#[tokio::test]
async fn a_failed_fetch_keeps_the_last_reading_and_degrades() {
    let mut harness = harness().await;
    call(&mut harness, Asked::Watch(at(47.3769, 8.5417)))
        .await
        .expect("a place to hold a reading");

    let generation = harness.service.generation;
    let fetch = fetched(&harness.service, vec![a_reading()]);
    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Fetched {
                generation,
                result: Ok(fetch),
            }),
        )
        .await;
    assert_eq!(harness.service.places.len(), 1);
    let updated_at = harness
        .service
        .updated_at
        .expect("a successful update time");

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Fetched {
                generation: harness.service.generation,
                result: Err("the request timed out".to_owned()),
            }),
        )
        .await;

    assert_eq!(harness.service.places.len(), 1, "the last reading is kept");
    assert_eq!(harness.service.updated_at, Some(updated_at));
    assert_eq!(
        reason(&harness.health).as_deref(),
        Some("the request timed out")
    );
}

#[tokio::test]
async fn an_unresolved_named_place_keeps_other_places_current() {
    let mut harness = harness().await;
    let valid = at(47.3769, 8.5417);
    call(&mut harness, Asked::Watch(valid.clone()))
        .await
        .expect("a fixed place");
    call(
        &mut harness,
        Asked::Watch(WatchedPlace::Location {
            name: "Missing, ZZ".to_owned(),
        }),
    )
    .await
    .expect("a syntactically valid named place");

    let fetch = Fetch {
        targets: vec![ResolvedTarget {
            place: valid.clone(),
            resolved: ResolvedPlace {
                coordinates: fix(47.3769, 8.5417),
                city: "Zurich".to_owned(),
                country_code: "CH".to_owned(),
            },
        }],
        readings: vec![a_reading()],
        failure: Some("no location matches the requested city and country".to_owned()),
    };
    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Fetched {
                generation: harness.service.generation,
                result: Ok(fetch),
            }),
        )
        .await;

    assert_eq!(
        harness
            .service
            .places
            .iter()
            .map(|place| place.place.clone())
            .collect::<Vec<_>>(),
        vec![valid]
    );
    assert_eq!(harness.health.borrow().clone(), ServiceState::Running);
}

#[tokio::test]
async fn an_unresolved_only_place_degrades_without_echoing_its_name() {
    let mut harness = harness().await;
    call(
        &mut harness,
        Asked::Watch(WatchedPlace::Location {
            name: "Private Place, ZZ".to_owned(),
        }),
    )
    .await
    .expect("a syntactically valid named place");

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Fetched {
                generation: harness.service.generation,
                result: Ok(Fetch {
                    targets: Vec::new(),
                    readings: Vec::new(),
                    failure: Some("no location matches the requested city and country".to_owned()),
                }),
            }),
        )
        .await;

    let failure = reason(&harness.health).expect("an unresolved-only fetch degrades");
    assert_eq!(
        failure,
        "no location matches the requested city and country"
    );
    assert!(!failure.contains("Private Place"));
}

/// The query string carries the user's latitude and longitude, so a reason that quoted the
/// request would be a location leak wherever it was pasted.
#[tokio::test]
async fn a_failure_reason_names_neither_the_host_nor_a_coordinate() {
    let mut harness = harness().await;
    call(&mut harness, Asked::Watch(at(47.3769, 8.5417)))
        .await
        .expect("a watched place");

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Fetched {
                generation: harness.service.generation,
                result: Err("the provider answered 500 Internal Server Error".to_owned()),
            }),
        )
        .await;

    let rendered = reason(&harness.health).expect("a degraded reason");
    for secret in ["open-meteo", "47.3769", "8.5417", "latitude"] {
        assert!(
            !rendered.contains(secret),
            "the reason must not name `{secret}`, got {rendered}"
        );
    }
}

/// The closure captured the request as it stood; a result built from an older one must not
/// overwrite the current places.
#[tokio::test]
async fn a_fetch_from_an_earlier_generation_is_dropped() {
    let mut harness = harness().await;
    call(&mut harness, Asked::Watch(at(47.3769, 8.5417)))
        .await
        .expect("a watched place");

    let stale = harness.service.generation;
    call(&mut harness, Asked::Refresh)
        .await
        .expect("refresh answers");
    assert!(harness.service.generation > stale);

    let fetch = fetched(&harness.service, vec![a_reading()]);
    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Fetched {
                generation: stale,
                result: Ok(fetch),
            }),
        )
        .await;

    assert!(
        harness.service.places.is_empty(),
        "a stale fetch must not be absorbed"
    );
}

/// A fix can be queued behind the change that stopped anyone watching `here`. Dropping the
/// subscription does not un-queue it, so the guard is on the model.
#[tokio::test]
async fn a_geolocation_update_arriving_after_the_last_here_lease_expired_is_ignored() {
    let mut harness = harness().await;
    let now = Instant::now();
    harness
        .service
        .lease(WatchedPlace::Here, now)
        .expect("the lease");
    assert!(harness.service.sweep(now + LEASE + Duration::from_secs(1)));

    let settled = harness.service.generation;
    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Located(Some(fix(47.3769, 8.5417)))),
        )
        .await;

    assert_eq!(harness.service.generation, settled);
    assert_eq!(harness.service.fix, None);
}

/// Places are paired with readings by position, so a response of the wrong length is not a
/// partial answer — it would hand one place another place's weather.
#[tokio::test]
async fn a_response_that_does_not_cover_every_place_is_refused() {
    let mut harness = harness().await;
    for pair in [(47.3769, 8.5417), (-33.8688, 151.2093)] {
        call(&mut harness, Asked::Watch(at(pair.0, pair.1)))
            .await
            .expect("a watched place");
    }

    let mut fetch = fetched(&harness.service, vec![a_reading()]);
    fetch.targets.push(ResolvedTarget {
        place: at(-33.8688, 151.2093),
        resolved: ResolvedPlace {
            coordinates: fix(-33.8688, 151.2093),
            city: "Sydney".to_owned(),
            country_code: "AU".to_owned(),
        },
    });
    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Fetched {
                generation: harness.service.generation,
                result: Ok(fetch),
            }),
        )
        .await;

    assert!(harness.service.places.is_empty());
    assert_eq!(
        reason(&harness.health).as_deref(),
        Some("the provider answered for a different set of places")
    );
}
