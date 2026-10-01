use super::*;

/// Without the threshold a fix that jitters by metres bumps the generation on every update and
/// refetches for ever.
#[tokio::test]
async fn a_fix_that_moves_less_than_a_kilometre_does_not_refetch() {
    let mut harness = harness().await;
    call(&mut harness, Asked::Watch(WatchedPlace::Here))
        .await
        .expect("here is watchable");

    let settled = harness.service.generation;

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Located(Some(fix(47.3769, 8.5417)))),
        )
        .await;
    let first = harness.service.generation;
    assert!(first > settled, "the first fix is a change");

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Located(Some(fix(47.3772, 8.5419)))),
        )
        .await;
    assert_eq!(harness.service.generation, first, "300 m is not a move");

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Located(Some(fix(47.3900, 8.5417)))),
        )
        .await;
    assert!(harness.service.generation > first, "1.5 km is a move");
}

/// Each step is far below the threshold, so an implementation comparing against the fix it last
/// *saw* would never move at all. Comparing against the one it last *accepted* crosses once.
#[tokio::test]
async fn drift_is_measured_from_the_fix_that_was_accepted() {
    let mut harness = harness().await;
    call(&mut harness, Asked::Watch(WatchedPlace::Here))
        .await
        .expect("here is watchable");

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Located(Some(fix(47.0, 8.0)))),
        )
        .await;
    let settled = harness.service.generation;

    for step in 1..=4 {
        harness
            .service
            .handle(
                &harness.ctx,
                Input::Event(Event::Located(Some(fix(
                    47.0 + 0.002 * f64::from(step),
                    8.0,
                )))),
            )
            .await;
    }

    assert_eq!(
        harness.service.generation, settled,
        "four steps of about 222 m stay inside the threshold"
    );
    assert_eq!(harness.service.fix, Some(fix(47.0, 8.0)));

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Located(Some(fix(47.01, 8.0)))),
        )
        .await;

    assert_eq!(
        harness.service.generation,
        settled + 1,
        "the walk crosses the threshold measured from the accepted fix"
    );
    assert_eq!(harness.service.fix, Some(fix(47.01, 8.0)));
}

#[test]
fn a_kilometre_is_a_kilometre() {
    let north = metres(&fix(0.0, 0.0), &fix(0.009, 0.0));
    assert!(
        (north - 1000.0).abs() < 5.0,
        "0.009 degrees of latitude is about a kilometre, got {north}"
    );
    assert_eq!(metres(&fix(51.5, -0.1), &fix(51.5, -0.1)), 0.0);
}
