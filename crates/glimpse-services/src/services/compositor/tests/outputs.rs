use super::*;

#[test]
fn a_disabled_output_still_publishes_with_enabled_false() {
    let state = Snapshot {
        outputs: vec![output("DP-1", true), output("HDMI-A-1", false)],
        focused_output: Some("DP-1".to_owned()),
        ..Snapshot::default()
    };

    let published = outputs_of(&state);

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

#[test]
fn a_snapshot_populates_every_output_field() {
    let mut display = output("DP-1", true);
    display.model = Some("ATNA60CL10-0".to_owned());
    display.serial = Some("69QC174".to_owned());
    display.current_mode = Some(glimpse_compositors::Mode {
        width: 3840,
        height: 2160,
        refresh_mhz: 239_991,
    });
    display.logical = Some(glimpse_compositors::Logical {
        x: -3072,
        y: 0,
        width: 3072,
        height: 1728,
        scale: 1.25,
    });
    let state = Snapshot {
        outputs: vec![display],
        focused_output: Some("DP-1".to_owned()),
        ..Snapshot::default()
    };

    let published = outputs_of(&state);

    assert_eq!(published[0].make.as_deref(), Some("Samsung"));
    assert_eq!(published[0].model.as_deref(), Some("ATNA60CL10-0"));
    assert_eq!(published[0].serial.as_deref(), Some("69QC174"));
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

#[test]
fn output_power_is_mirrored_into_compositor_capabilities() {
    let mapped = capabilities(Capabilities {
        output_power: true,
        blank_screens: true,
        window_screenshot: false,
        overview_backdrop: true,
    });

    assert!(mapped.output_power && mapped.blank_screens);
}

#[tokio::test]
async fn an_identical_snapshot_does_not_republish() {
    let mut harness = harness(Backend::Unsupported);
    let snapshot = || Snapshot {
        outputs: vec![output("DP-1", true), output("HDMI-A-1", false)],
        focused_output: Some("DP-1".to_owned()),
        ..Snapshot::default()
    };

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Snapshot(Box::new(snapshot()))),
        )
        .await;
    harness.state.borrow_and_update();

    harness
        .service
        .handle(
            &harness.ctx,
            Input::Event(Event::Snapshot(Box::new(snapshot()))),
        )
        .await;

    assert!(
        !harness.state.has_changed().expect("sender is alive"),
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
