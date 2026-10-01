use super::*;

#[test]
fn a_capability_a_compositor_lacks_is_refused_without_inviting_a_retry() {
    assert!(matches!(
        command_error(&CompositorError::Unavailable("reorder a workspace")),
        CommandError::Unsupported(_)
    ));
    assert!(matches!(
        command_error(&CompositorError::Closed),
        CommandError::Unavailable(_)
    ));
}

#[tokio::test]
async fn disabling_the_only_enabled_output_is_refused_without_reaching_the_backend() {
    let fake = FakeNiri::spawn();
    let mut harness = harness(Backend::Niri(Niri::at(fake.socket.clone())));
    snapshot_of_outputs(&mut harness, vec![output("DP-1", true)]).await;

    let result = set_output_enabled(&mut harness, "DP-1", false).await;

    assert!(matches!(result, Err(CommandError::InvalidArgument(_))));
    assert!(
        fake.requests().is_empty(),
        "no call may reach the compositor backend"
    );
}

#[tokio::test]
async fn disabling_one_of_two_enabled_outputs_reaches_the_backend_and_succeeds() {
    let fake = FakeNiri::spawn();
    let mut harness = harness(Backend::Niri(Niri::at(fake.socket.clone())));
    snapshot_of_outputs(
        &mut harness,
        vec![output("DP-1", true), output("HDMI-A-1", true)],
    )
    .await;

    let result = set_output_enabled(&mut harness, "DP-1", false).await;

    assert_eq!(result, Ok(()));
    assert_eq!(
        fake.requests(),
        vec![r#"{"Output":{"action":"Off","output":"DP-1"}}"#]
    );
}

#[tokio::test]
async fn enabling_a_disabled_output_is_never_blocked_by_the_guard() {
    let fake = FakeNiri::spawn();
    let mut harness = harness(Backend::Niri(Niri::at(fake.socket.clone())));
    snapshot_of_outputs(
        &mut harness,
        vec![output("DP-1", true), output("HDMI-A-1", false)],
    )
    .await;

    let result = set_output_enabled(&mut harness, "HDMI-A-1", true).await;

    assert_eq!(result, Ok(()));
    assert_eq!(
        fake.requests(),
        vec![r#"{"Output":{"action":"On","output":"HDMI-A-1"}}"#]
    );
}

#[tokio::test]
async fn disabling_an_already_disabled_output_is_not_mistaken_for_the_last_one() {
    let fake = FakeNiri::spawn();
    let mut harness = harness(Backend::Niri(Niri::at(fake.socket.clone())));
    snapshot_of_outputs(
        &mut harness,
        vec![output("DP-1", true), output("HDMI-A-1", false)],
    )
    .await;

    let result = set_output_enabled(&mut harness, "HDMI-A-1", false).await;

    assert_eq!(result, Ok(()));
    assert_eq!(
        fake.requests(),
        vec![r#"{"Output":{"action":"Off","output":"HDMI-A-1"}}"#]
    );
}

#[tokio::test]
async fn an_unknown_connector_is_refused_without_reaching_the_backend() {
    let fake = FakeNiri::spawn();
    let mut harness = harness(Backend::Niri(Niri::at(fake.socket.clone())));
    snapshot_of_outputs(&mut harness, vec![output("DP-1", true)]).await;

    let result = set_output_enabled(&mut harness, "HDMI-A-1", false).await;

    assert!(matches!(result, Err(CommandError::InvalidArgument(_))));
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn power_off_monitors_reaches_the_backend_with_no_last_output_guard() {
    let fake = FakeNiri::spawn();
    let mut harness = harness(Backend::Niri(Niri::at(fake.socket.clone())));
    snapshot_of_outputs(&mut harness, vec![output("DP-1", true)]).await;

    let result = power_off_monitors(&mut harness).await;

    assert_eq!(result, Ok(()));
    assert_eq!(
        fake.requests(),
        vec![r#"{"Action":{"PowerOffMonitors":{}}}"#]
    );
}

#[tokio::test]
async fn stop_screencast_reaches_the_backend_with_the_session_id() {
    let fake = FakeNiri::spawn();
    let mut harness = harness(Backend::Niri(Niri::at(fake.socket.clone())));

    let result = stop_screencast(&mut harness, 2).await;

    assert_eq!(result, Ok(()));
    assert_eq!(
        fake.requests(),
        vec![r#"{"Action":{"StopCast":{"session_id":2}}}"#]
    );
}

#[tokio::test]
async fn stop_screencast_is_unavailable_under_hyprland() {
    let mut harness = harness(Backend::Hyprland(glimpse_compositors::Hyprland::at(
        "/nonexistent",
    )));

    assert!(matches!(
        stop_screencast(&mut harness, 2).await,
        Err(CommandError::Unsupported(_))
    ));
}

#[tokio::test]
async fn output_commands_are_unsupported_without_the_capability() {
    let mut harness = harness(Backend::Unsupported);
    snapshot_of_outputs(
        &mut harness,
        vec![output("DP-1", true), output("HDMI-A-1", true)],
    )
    .await;

    assert!(matches!(
        set_output_enabled(&mut harness, "DP-1", false).await,
        Err(CommandError::Unsupported(_))
    ));
    assert!(matches!(
        power_off_monitors(&mut harness).await,
        Err(CommandError::Unsupported(_))
    ));
}

#[tokio::test]
async fn set_output_enabled_before_any_snapshot_is_refused_rather_than_reaching_the_backend() {
    let fake = FakeNiri::spawn();
    let mut harness = harness(Backend::Niri(Niri::at(fake.socket.clone())));

    let result = set_output_enabled(&mut harness, "DP-1", false).await;

    assert_eq!(
        result,
        Err(CommandError::InvalidArgument(
            "the compositor state has not arrived yet".to_owned()
        ))
    );
    assert!(fake.requests().is_empty());
}
