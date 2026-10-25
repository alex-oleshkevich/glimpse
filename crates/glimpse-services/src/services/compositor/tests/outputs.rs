use super::*;

fn outputs(rig: &Rig) -> Vec<OutputInfo> {
    rig.mirror
        .handle
        .snapshot()
        .outputs
        .expect("outputs")
        .outputs
}

#[tokio::test]
async fn a_disabled_output_still_publishes_with_enabled_false() {
    let rig = Rig::start(Snapshot {
        outputs: vec![output("DP-1", true), output("HDMI-A-1", false)],
        focus: focus(Some("DP-1"), None),
        ..Snapshot::default()
    })
    .await;

    let published = outputs(&rig);

    assert_eq!(published.len(), 2, "a disabled output must stay switchable");
    let disabled = published
        .iter()
        .find(|output| output.connector == "HDMI-A-1")
        .expect("HDMI-A-1");
    assert!(!disabled.enabled);
    let enabled = published
        .iter()
        .find(|output| output.connector == "DP-1")
        .expect("DP-1");
    assert!(enabled.focused);
    assert_eq!(
        enabled.label.as_deref(),
        Some("Samsung ATNA60CL10-0"),
        "niri fills make and model, leaves description null, and pads the model with a \
             trailing space; a popover offering `Move to display` has nothing to render otherwise"
    );
}

#[tokio::test]
async fn a_snapshot_populates_every_output_field() {
    let mut display = output("DP-1", true);
    display.model = Some("ATNA60CL10-0".to_owned());
    display.mode = Some(Mode {
        width: 3840,
        height: 2160,
        refresh_mhz: 239_991,
    });
    display.logical = Some(Logical {
        x: -3072,
        y: 0,
        width: 3072,
        height: 1728,
        scale: 1.25,
    });
    let rig = Rig::start(Snapshot {
        outputs: vec![display],
        focus: focus(Some("DP-1"), None),
        ..Snapshot::default()
    })
    .await;

    let published = outputs(&rig);

    assert_eq!(published[0].make.as_deref(), Some("Samsung"));
    assert_eq!(published[0].model.as_deref(), Some("ATNA60CL10-0"));
    assert_eq!(
        published[0].current_mode,
        Some(OutputMode {
            width: 3840,
            height: 2160,
            refresh_mhz: 239_991,
        })
    );
    assert_eq!(
        published[0].logical,
        Some(OutputLogical {
            x: -3072,
            y: 0,
            width: 3072,
            height: 1728,
            scale: 1.25,
        })
    );
    assert!(published[0].enabled);
}

#[tokio::test]
async fn capabilities_are_mirrored_into_the_status_and_supports() {
    let rig = Rig::start(Snapshot::default()).await;

    let status = rig.mirror.handle.snapshot().status.expect("status");
    assert!(status.capabilities.contains(&Capability::OutputPower));
    assert!(status.capabilities.contains(&Capability::BlankScreens));
    assert!(rig.mirror.handle.supports(Capability::OutputPower));
    assert!(!rig.mirror.handle.supports(Capability::WindowScreenshot));
}

#[tokio::test]
async fn built_in_is_set_only_for_edp_and_lvds_connectors() {
    let rig = Rig::start(Snapshot {
        outputs: vec![
            output("eDP-1", true),
            output("DP-2", true),
            output("LVDS-1", true),
        ],
        ..Snapshot::default()
    })
    .await;

    let built_in: Vec<_> = outputs(&rig)
        .into_iter()
        .map(|output| (output.connector, output.built_in))
        .collect();

    assert_eq!(
        built_in,
        [
            ("eDP-1".to_owned(), true),
            ("DP-2".to_owned(), false),
            ("LVDS-1".to_owned(), true),
        ]
    );
}

#[tokio::test]
async fn an_identical_snapshot_does_not_republish() {
    let mut rig = Rig::start(Snapshot {
        outputs: vec![output("DP-1", true), output("HDMI-A-1", false)],
        focus: focus(Some("DP-1"), None),
        ..Snapshot::default()
    })
    .await;
    rig.state.borrow_and_update();

    rig.send(BackendEvent::Resync(
        glimpse_compositors::event::Resync::Outputs,
    ));

    assert!(
        tokio::time::timeout(Duration::from_millis(500), rig.state.changed())
            .await
            .is_err(),
        "two identical snapshots must not rebuild every consumer's widget list"
    );
}

#[test]
fn a_description_is_preferred_to_a_make_and_model() {
    let mut described = output("DP-1", true);
    described.description = Some("Samsung 14\"".to_owned());

    assert_eq!(
        label_of(&described).as_deref(),
        Some("Samsung 14\""),
        "Hyprland supplies the description, which is the better label of the two"
    );
}
