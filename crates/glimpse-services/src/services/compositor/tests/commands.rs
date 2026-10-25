use super::*;

fn two_outputs() -> Snapshot {
    Snapshot {
        outputs: vec![output("DP-1", true), output("HDMI-A-1", true)],
        ..Snapshot::default()
    }
}

#[test]
fn provider_errors_map_to_command_errors_without_inviting_a_retry() {
    assert!(matches!(
        command_error(&ProviderError::Unsupported("stop a screencast".to_owned())),
        CommandError::Unsupported(_)
    ));
    assert!(matches!(
        command_error(&ProviderError::Unavailable("gone".to_owned())),
        CommandError::Unavailable(_)
    ));
    assert!(matches!(
        command_error(&ProviderError::Refused("no".to_owned())),
        CommandError::InvalidArgument(_)
    ));
    assert!(matches!(
        command_error(&ProviderError::Failed("boom".to_owned())),
        CommandError::Internal(_)
    ));
}

#[tokio::test]
async fn disabling_the_only_enabled_output_is_refused_without_reaching_the_backend() {
    let rig = Rig::start(Snapshot {
        outputs: vec![output("DP-1", true)],
        ..Snapshot::default()
    })
    .await;

    let result = rig
        .mirror
        .handle
        .set_output_enabled("DP-1".to_owned(), false)
        .await;

    assert!(matches!(result, Err(CommandError::InvalidArgument(_))));
    assert!(rig.calls().is_empty(), "no call may reach the backend");
}

#[tokio::test]
async fn disabling_one_of_two_enabled_outputs_reaches_the_backend_and_succeeds() {
    let rig = Rig::start(two_outputs()).await;

    let result = rig
        .mirror
        .handle
        .set_output_enabled("DP-1".to_owned(), false)
        .await;

    assert_eq!(result, Ok(()));
    assert_eq!(rig.calls(), ["set_output_enabled DP-1 false"]);
}

#[tokio::test]
async fn enabling_a_disabled_output_is_never_blocked_by_the_guard() {
    let rig = Rig::start(Snapshot {
        outputs: vec![output("DP-1", true), output("HDMI-A-1", false)],
        ..Snapshot::default()
    })
    .await;

    let result = rig
        .mirror
        .handle
        .set_output_enabled("HDMI-A-1".to_owned(), true)
        .await;

    assert_eq!(result, Ok(()));
    assert_eq!(rig.calls(), ["set_output_enabled HDMI-A-1 true"]);
}

#[tokio::test]
async fn disabling_an_already_disabled_output_is_not_mistaken_for_the_last_one() {
    let rig = Rig::start(Snapshot {
        outputs: vec![output("DP-1", true), output("HDMI-A-1", false)],
        ..Snapshot::default()
    })
    .await;

    let result = rig
        .mirror
        .handle
        .set_output_enabled("HDMI-A-1".to_owned(), false)
        .await;

    assert_eq!(result, Ok(()));
    assert_eq!(rig.calls(), ["set_output_enabled HDMI-A-1 false"]);
}

#[tokio::test]
async fn an_unknown_connector_is_refused_without_reaching_the_backend() {
    let rig = Rig::start(Snapshot {
        outputs: vec![output("DP-1", true)],
        ..Snapshot::default()
    })
    .await;

    let result = rig
        .mirror
        .handle
        .set_output_enabled("HDMI-A-1".to_owned(), false)
        .await;

    assert!(matches!(result, Err(CommandError::InvalidArgument(_))));
    assert!(rig.calls().is_empty());
}

#[tokio::test]
async fn power_off_monitors_reaches_the_backend_with_no_last_output_guard() {
    let rig = Rig::start(Snapshot {
        outputs: vec![output("DP-1", true)],
        ..Snapshot::default()
    })
    .await;

    let result = rig.mirror.handle.power_off_monitors().await;

    assert_eq!(result, Ok(()));
    assert_eq!(rig.calls(), ["power_off_monitors"]);
}

#[tokio::test]
async fn stop_screencast_reaches_the_backend_with_the_session_id() {
    let mut known = cast(1, true);
    known.session = Some(2);
    let rig = Rig::start(Snapshot {
        casts: vec![known],
        ..Snapshot::default()
    })
    .await;

    let result = rig.mirror.handle.stop_screencast(2).await;

    assert_eq!(result, Ok(()));
    assert_eq!(rig.calls(), ["stop_screencast 2"]);
}

#[tokio::test]
async fn stop_screencast_for_an_unknown_session_is_an_invalid_argument() {
    let rig = Rig::start(Snapshot::default()).await;

    let result = rig.mirror.handle.stop_screencast(2).await;

    assert!(matches!(result, Err(CommandError::InvalidArgument(_))));
    assert!(rig.calls().is_empty());
}

#[tokio::test]
async fn blanking_the_screens_is_refused_without_the_capability() {
    let rig = Rig::with::<RenameOnly>(Snapshot {
        outputs: vec![output("DP-1", true)],
        ..Snapshot::default()
    })
    .await;

    assert!(matches!(
        rig.mirror.handle.power_off_monitors().await,
        Err(CommandError::Unsupported(_))
    ));
    assert!(rig.calls().is_empty());
}

#[tokio::test]
async fn output_commands_are_unsupported_without_the_capability() {
    let rig = Rig::with::<Nothing>(two_outputs()).await;

    assert!(matches!(
        rig.mirror
            .handle
            .set_output_enabled("DP-1".to_owned(), false)
            .await,
        Err(CommandError::Unsupported(_))
    ));
    assert!(matches!(
        rig.mirror.handle.power_off_monitors().await,
        Err(CommandError::Unsupported(_))
    ));
    assert!(rig.calls().is_empty());
}

#[tokio::test]
async fn renaming_a_workspace_is_gated_on_the_capability() {
    let snapshot = || snapshot_of(vec![workspace(1, Some(1), "DP-1")], Vec::new());
    let without = Rig::with::<Nothing>(snapshot()).await;
    let with = Rig::start(snapshot()).await;

    assert!(matches!(
        without
            .mirror
            .handle
            .rename_workspace(1, Some("web".to_owned()))
            .await,
        Err(CommandError::Unsupported(_))
    ));
    assert!(without.calls().is_empty());
    assert_eq!(
        with.mirror
            .handle
            .rename_workspace(1, Some("web".to_owned()))
            .await,
        Ok(())
    );
    assert_eq!(with.calls(), ["rename_workspace 1 web"]);
}
