use super::*;

#[tokio::test]
async fn a_place_whose_lease_expired_leaves_the_published_list() {
    let mut harness = harness().await;
    let now = Instant::now();
    harness
        .service
        .lease(at(47.3769, 8.5417), now)
        .expect("the lease");

    let fetch = fetched(&harness.service, vec![a_reading()]);
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
    assert_eq!(harness.service.places.len(), 1);

    harness.service.watched[0].until = now;
    let fetch = fetched(&harness.service, Vec::new());
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

    assert!(harness.service.watched.is_empty());
    assert!(harness.service.places.is_empty());
}

#[tokio::test]
async fn a_fix_that_moved_drops_only_the_place_that_followed_it() {
    let mut harness = harness().await;
    let now = Instant::now();

    harness
        .service
        .lease(at(47.3769, 8.5417), now)
        .expect("a place named by coordinate");
    harness
        .service
        .lease(WatchedPlace::Here, now)
        .expect("a place that follows the fix");
    harness.service.fix = Some(fix(51.5, -0.1));

    let fetch = fetched(&harness.service, vec![a_reading(), a_reading()]);
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
    assert_eq!(harness.service.places.len(), 2);

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Located(Some(fix(52.0, -0.1)))),
        )
        .await;

    assert_eq!(
        harness
            .service
            .places
            .iter()
            .map(|shown| shown.place.clone())
            .collect::<Vec<_>>(),
        vec![at(47.3769, 8.5417)]
    );
}
