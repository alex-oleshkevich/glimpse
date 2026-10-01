use super::*;

#[test]
fn an_inactive_cast_is_excluded() {
    let privacy = CompositorPrivacy {
        active: false,
        casts: vec![cast(
            1,
            Some(1),
            CastKindInfo::PipeWire,
            CastTargetInfo::Unknown,
            false,
        )],
    };

    assert!(screen(&privacy, &HashMap::new()).is_empty());
}

#[test]
fn both_cast_kinds_light_the_indicator_identically() {
    let privacy = CompositorPrivacy {
        active: true,
        casts: vec![
            cast(
                1,
                Some(11),
                CastKindInfo::PipeWire,
                CastTargetInfo::Output("DP-2".to_owned()),
                true,
            ),
            cast(
                2,
                None,
                CastKindInfo::Screencopy,
                CastTargetInfo::Unknown,
                true,
            ),
        ],
    };

    let facts = screen(&privacy, &HashMap::new());
    assert_eq!(facts.len(), 2, "neither cast kind is treated as invisible");
    assert_eq!(facts[0].detail.as_deref(), Some("DP-2"));
    assert_eq!(facts[0].app, None);
    assert_eq!(facts[0].stream_id, Some(1));
    assert_eq!(facts[1].detail, None);
    assert_eq!(
        facts[1].app, None,
        "a screencopy cast has no pipewire node to attribute"
    );
    assert_eq!(facts[1].stream_id, Some(2));
}

#[test]
fn a_pipewire_cast_is_named_via_its_active_link_consumer() {
    let privacy = CompositorPrivacy {
        active: true,
        casts: vec![CastInfo {
            pw_node_id: Some(107),
            ..cast(
                1,
                Some(11),
                CastKindInfo::PipeWire,
                CastTargetInfo::Unknown,
                true,
            )
        }],
    };
    let attribution = HashMap::from([(107, "chrome".to_owned())]);

    assert_eq!(
        screen(&privacy, &attribution)[0].app.as_deref(),
        Some("chrome")
    );
}

#[test]
fn a_screencopy_cast_is_never_attributed_even_carrying_a_stray_pw_node_id() {
    let privacy = CompositorPrivacy {
        active: true,
        casts: vec![CastInfo {
            pw_node_id: Some(107),
            ..cast(
                1,
                None,
                CastKindInfo::Screencopy,
                CastTargetInfo::Unknown,
                true,
            )
        }],
    };
    let attribution = HashMap::from([(107, "chrome".to_owned())]);

    assert_eq!(
        screen(&privacy, &attribution)[0].app,
        None,
        "attribution is scoped to CastKindInfo::PipeWire, never joined on pw_node_id alone"
    );
}

#[test]
fn a_pipewire_cast_with_no_matching_link_stays_unnamed() {
    let privacy = CompositorPrivacy {
        active: true,
        casts: vec![CastInfo {
            pw_node_id: Some(107),
            ..cast(
                1,
                Some(11),
                CastKindInfo::PipeWire,
                CastTargetInfo::Unknown,
                true,
            )
        }],
    };

    assert_eq!(screen(&privacy, &HashMap::new())[0].app, None);
}

#[test]
fn only_a_pipewire_cast_with_a_session_id_carries_a_stoppable_session() {
    let privacy = CompositorPrivacy {
        active: true,
        casts: vec![
            cast(
                1,
                Some(11),
                CastKindInfo::PipeWire,
                CastTargetInfo::Unknown,
                true,
            ),
            cast(
                2,
                Some(22),
                CastKindInfo::Screencopy,
                CastTargetInfo::Unknown,
                true,
            ),
            cast(
                3,
                None,
                CastKindInfo::PipeWire,
                CastTargetInfo::Unknown,
                true,
            ),
        ],
    };

    let facts = screen(&privacy, &HashMap::new());
    assert_eq!(
        facts[0].session,
        Some(11),
        "a PipeWire cast with a session id can be stopped"
    );
    assert_eq!(
        facts[1].session, None,
        "niri cannot stop a wlr-screencopy cast; offering the control would lie"
    );
    assert_eq!(
        facts[2].session, None,
        "Hyprland's synthetic casts carry no session id at all"
    );
}

#[test]
fn a_window_target_carries_no_detail() {
    let privacy = CompositorPrivacy {
        active: true,
        casts: vec![cast(
            1,
            Some(1),
            CastKindInfo::PipeWire,
            CastTargetInfo::Window(9),
            true,
        )],
    };

    assert_eq!(screen(&privacy, &HashMap::new())[0].detail, None);
}
