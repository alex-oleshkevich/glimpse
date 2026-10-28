use super::*;

async fn unowned() -> (PrivateBus, Mirror) {
    let bus = PrivateBus::start();
    let mirror = mirror_on(&bus).await;
    let mut state = mirror.handle.subscribe();
    until(&mut state, "the unsupported status", |state| {
        state.status.is_some()
    })
    .await;
    let mut health = mirror.handle.health();
    until(&mut health, "running health", |health| {
        *health == ServiceState::Running
    })
    .await;
    (bus, mirror)
}

#[tokio::test]
async fn a_bus_with_no_owner_publishes_the_unsupported_status_and_stays_running() {
    let (_bus, mirror) = unowned().await;

    let published = mirror.handle.snapshot();
    let status = published.status.expect("a status");
    assert_eq!(status.name, "unsupported");
    assert!(status.capabilities.is_empty());
    assert!(published.workspaces.is_none());
    assert!(published.windows.is_none());
    assert!(published.outputs.is_none());
    assert_eq!(
        published.privacy,
        Some(CompositorPrivacy {
            active: false,
            casts: Vec::new()
        })
    );
    assert_eq!(health_of(&mirror), ServiceState::Running);
}

#[tokio::test]
async fn every_command_is_unsupported_locally_while_no_provider_ever_owned_the_name() {
    let (_bus, mirror) = unowned().await;
    let handle = &mirror.handle;

    let unsupported = |result: Result<(), CommandError>| {
        assert!(
            matches!(result, Err(CommandError::Unsupported(_))),
            "{result:?}"
        )
    };
    unsupported(handle.focus_workspace(WorkspaceRef::Next).await);
    unsupported(handle.focus_window(WindowRef::Prev).await);
    unsupported(handle.focus_app(Some(1), &["app"]).await);
    unsupported(handle.rename_workspace(1, None).await);
    unsupported(handle.set_output_enabled("DP-1".to_owned(), false).await);
    unsupported(handle.power_off_monitors().await);
    unsupported(handle.stop_screencast(1).await);
}

#[tokio::test]
async fn an_owner_that_appears_later_is_followed() {
    let (bus, mirror) = unowned().await;
    let mut state = mirror.handle.subscribe();

    let _served = serve_on::<Everything>(
        &bus,
        snapshot_of(vec![workspace(1, Some(1), "DP-2")], Vec::new()),
    )
    .await;
    let published = until(&mut state, "the provider's sections", |state| {
        state.workspaces.is_some()
    })
    .await;

    let status = published.status.expect("a status");
    assert_eq!(status.name, "fake");
    assert_eq!(status.capabilities, Everything::SET);
    assert_eq!(
        published.workspaces.map(|shown| shown.workspaces.len()),
        Some(1)
    );
    assert_eq!(health_of(&mirror), ServiceState::Running);
}

#[tokio::test]
async fn a_lost_owner_degrades_keeps_the_last_state_and_a_new_owner_recovers() {
    let mut rig = Rig::start(snapshot_of(vec![workspace(1, Some(1), "DP-2")], Vec::new())).await;
    let before = rig.mirror.handle.snapshot();
    let mut health = rig.mirror.handle.health();
    until(&mut health, "running health", |health| {
        *health == ServiceState::Running
    })
    .await;

    rig.served.task.abort();
    until(&mut health, "degraded health", |health| {
        matches!(health, ServiceState::Degraded { .. })
    })
    .await;

    assert_eq!(rig.mirror.handle.snapshot(), before);
    assert!(matches!(
        rig.mirror
            .handle
            .focus_workspace(WorkspaceRef::Id { id: 1 })
            .await,
        Err(CommandError::Unavailable(_))
    ));

    let _replacement = serve_on::<RenameOnly>(
        &rig.bus,
        snapshot_of(vec![workspace(1, Some(1), "DP-2")], Vec::new()),
    )
    .await;
    until(&mut health, "running health again", |health| {
        *health == ServiceState::Running
    })
    .await;
    rig.until("the new capabilities", |state| {
        state
            .status
            .as_ref()
            .is_some_and(|status| status.capabilities == RenameOnly::SET)
    })
    .await;
}

#[tokio::test]
async fn privacy_stays_unpublished_until_the_provider_ownership_resolves() {
    let cancel = tokio_util::sync::CancellationToken::new();
    let (events, _inbox) = mpsc::channel(8);
    let (published, state) = watch::channel(CompositorState::default());
    let (health, _health) = watch::channel(ServiceState::Starting);
    let ctx = Ctx::<Compositor>::new(
        events,
        &cancel,
        published,
        health,
        Buses::unavailable("no bus in tests"),
    );
    let bus = PrivateBus::start();
    let provider = glimpse_compositors::client::CompositorProvider::start(bus.connection().await);
    let mut service = Compositor::start(
        &ctx,
        NoConfig,
        Dependencies {
            provider: ProviderHandle(provider.handle()),
        },
    )
    .await;
    let lost = |reason: &str| {
        Input::Event(Event::Provider(Box::new(Mirrored {
            unavailable: Some(reason.to_owned()),
            ..Mirrored::default()
        })))
    };

    service.handle(&ctx, lost(glimpse_dbus::PENDING)).await;
    assert_eq!(*state.borrow(), CompositorState::default());

    service
        .handle(&ctx, lost("provider has no bus owner"))
        .await;
    let resolved = state.borrow().clone();
    assert_eq!(
        resolved.status.map(|status| status.name).as_deref(),
        Some("unsupported")
    );
    assert_eq!(
        resolved.privacy,
        Some(CompositorPrivacy {
            active: false,
            casts: Vec::new()
        })
    );
}
