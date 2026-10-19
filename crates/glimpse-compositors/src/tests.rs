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
        compositor.keyboard_layouts().await,
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
fn blank_screens_is_niri_only_and_output_power_is_on_both() {
    let niri = Compositor::Niri(Niri::at("/nonexistent")).capabilities();
    let hyprland = Compositor::Hyprland(Hyprland::at("/nonexistent")).capabilities();

    assert!(niri.output_power && hyprland.output_power);
    assert!(niri.blank_screens && !hyprland.blank_screens);
}

#[tokio::test]
async fn a_compositor_without_the_restore_option_allows_a_restarted_locker() {
    assert!(
        Compositor::Unsupported
            .allows_session_lock_restore()
            .await
            .unwrap()
    );
    assert!(
        Compositor::Niri(Niri::at("/nonexistent"))
            .allows_session_lock_restore()
            .await
            .unwrap()
    );
}
