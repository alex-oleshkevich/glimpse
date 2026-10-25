use super::*;

#[test]
fn an_inactive_cast_is_excluded() {
    let privacy = CompositorPrivacy {
        active: false,
        casts: vec![cast(1, Some(1), None, false)],
    };

    assert!(screen(&privacy, &HashMap::new()).is_empty());
}

#[test]
fn a_cast_with_and_without_a_session_light_the_indicator_identically() {
    let privacy = CompositorPrivacy {
        active: true,
        casts: vec![
            cast(1, Some(11), Some("DP-2"), true),
            cast(2, None, None, true),
        ],
    };

    let facts = screen(&privacy, &HashMap::new());
    assert_eq!(facts.len(), 2, "neither cast shape is treated as invisible");
    assert_eq!(facts[0].detail.as_deref(), Some("DP-2"));
    assert_eq!(facts[0].app, None);
    assert_eq!(facts[0].stream_id, Some(1));
    assert_eq!(facts[1].detail, None);
    assert_eq!(
        facts[1].app, None,
        "a cast with no pipewire node has nothing to attribute"
    );
    assert_eq!(facts[1].stream_id, Some(2));
}

#[test]
fn a_cast_with_a_node_is_named_via_its_active_link_consumer() {
    let privacy = CompositorPrivacy {
        active: true,
        casts: vec![CastInfo {
            pw_node_id: Some(107),
            ..cast(1, Some(11), None, true)
        }],
    };
    let attribution = HashMap::from([(107, "chrome".to_owned())]);

    assert_eq!(
        screen(&privacy, &attribution)[0].app.as_deref(),
        Some("chrome")
    );
}

#[test]
fn a_cast_without_a_session_is_never_attributed_even_carrying_a_stray_node() {
    let privacy = CompositorPrivacy {
        active: true,
        casts: vec![CastInfo {
            pw_node_id: Some(107),
            ..cast(1, None, None, true)
        }],
    };
    let attribution = HashMap::from([(107, "chrome".to_owned())]);

    assert_eq!(
        screen(&privacy, &attribution)[0].app,
        None,
        "attribution is scoped to a cast with a session, never joined on pw_node_id alone"
    );
}

#[test]
fn a_cast_without_a_node_is_never_attributed() {
    let privacy = CompositorPrivacy {
        active: true,
        casts: vec![cast(1, None, None, true)],
    };
    let attribution = HashMap::from([(107, "chrome".to_owned())]);

    assert_eq!(
        screen(&privacy, &attribution)[0].app,
        None,
        "attribution joins on the cast's own pw_node_id, never on another stream's"
    );
}

#[test]
fn a_cast_with_no_matching_link_stays_unnamed() {
    let privacy = CompositorPrivacy {
        active: true,
        casts: vec![CastInfo {
            pw_node_id: Some(107),
            ..cast(1, Some(11), None, true)
        }],
    };

    assert_eq!(screen(&privacy, &HashMap::new())[0].app, None);
}

#[test]
fn only_a_cast_with_a_session_id_carries_a_stoppable_session() {
    let privacy = CompositorPrivacy {
        active: true,
        casts: vec![cast(1, Some(11), None, true), cast(3, None, None, true)],
    };

    let facts = screen(&privacy, &HashMap::new());
    assert_eq!(
        facts[0].session,
        Some(11),
        "a cast with a session id can be stopped"
    );
    assert_eq!(
        facts[1].session, None,
        "a cast with no session id offers no stop control"
    );
}

#[test]
fn a_cast_without_an_output_carries_no_detail() {
    let privacy = CompositorPrivacy {
        active: true,
        casts: vec![cast(1, Some(1), None, true)],
    };

    assert_eq!(screen(&privacy, &HashMap::new())[0].detail, None);
}

#[test]
fn a_cast_with_an_output_and_a_node_carries_session_detail_and_app() {
    let privacy = CompositorPrivacy {
        active: true,
        casts: vec![
            CastInfo {
                pw_node_id: Some(42),
                ..cast(1, Some(5), Some("DP-2"), true)
            },
            cast(2, None, None, true),
        ],
    };
    let attribution = HashMap::from([(42, "obs".to_owned())]);

    let facts = screen(&privacy, &attribution);

    assert_eq!(facts[0].session, Some(5));
    assert_eq!(facts[0].detail.as_deref(), Some("DP-2"));
    assert_eq!(facts[0].app.as_deref(), Some("obs"));
    assert_eq!(facts[1].session, None);
    assert_eq!(facts[1].app, None);
}
