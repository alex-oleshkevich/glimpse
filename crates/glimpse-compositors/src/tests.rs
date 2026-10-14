use super::*;

#[tokio::test]
async fn an_unsupported_session_answers_every_operation_rather_than_panicking() {
    let compositor = Compositor::Unsupported;

    assert_eq!(compositor.name(), "unsupported");
    assert_eq!(compositor.capabilities(), Capabilities::NONE);
    assert!(matches!(
        compositor.snapshot().await,
        Err(CompositorError::Unsupported(_))
    ));
    assert!(matches!(
        compositor.events().await.err(),
        Some(CompositorError::Unsupported(_))
    ));
    assert!(matches!(
        compositor.focus_window(WindowTarget::Next).await,
        Err(CompositorError::Unsupported(_))
    ));
    assert!(matches!(
        compositor.set_output_enabled("eDP-1", true).await,
        Err(CompositorError::Unsupported(_))
    ));
    assert!(matches!(
        compositor.power_off_monitors().await,
        Err(CompositorError::Unsupported(_))
    ));
    assert!(matches!(
        compositor.close_window(WindowId(1)).await,
        Err(CompositorError::Unsupported(_))
    ));
    assert!(matches!(
        compositor.reorder_workspace(WorkspaceId(1), 0).await,
        Err(CompositorError::Unsupported(_))
    ));
    assert!(matches!(
        compositor.focus_output("eDP-1").await,
        Err(CompositorError::Unsupported(_))
    ));
    assert!(matches!(
        compositor.stop_screencast(2).await,
        Err(CompositorError::Unsupported(_))
    ));
    assert!(matches!(
        compositor
            .screenshot_window(WindowId(1), Path::new("/tmp/shot.png"))
            .await,
        Err(CompositorError::Unsupported(_))
    ));
}

#[test]
fn the_two_backends_disagree_about_floating_and_about_reordering() {
    let niri = Compositor::Niri(Niri::at("/nonexistent")).capabilities();
    let hyprland = Compositor::Hyprland(Hyprland::at("/nonexistent")).capabilities();

    assert!(!niri.floating && niri.workspace_reorder);
    assert!(
        hyprland.floating && !hyprland.workspace_reorder,
        "a Hyprland workspace's index is its identity, so there is no position to change"
    );
    assert!(niri.output_power && hyprland.output_power);
    assert!(niri.blank_screens && !hyprland.blank_screens);
}
