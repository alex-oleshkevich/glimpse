use super::*;
#[tokio::test]
async fn setup_timeout_surfaces_error_for_a_stalled_connect() {
    let stalled = std::future::pending::<anyhow::Result<()>>();
    let error = with_timeout(Duration::from_millis(10), stalled)
        .await
        .expect_err("a never-completing setup must time out");
    assert!(
        error.to_string().contains("timed out"),
        "unexpected error message: {error}"
    );
}

#[test]
fn the_session_display_comes_from_the_manager_environment() {
    let environment = |entries: &[&str]| entries.iter().map(|e| e.to_string()).collect::<Vec<_>>();
    assert_eq!(
        wayland_display(&environment(&["HOME=/home/a", "WAYLAND_DISPLAY=wayland-1"])),
        Some("wayland-1".to_owned())
    );
    assert_eq!(wayland_display(&environment(&["WAYLAND_DISPLAY="])), None);
    assert_eq!(wayland_display(&environment(&["HOME=/home/a"])), None);
}

#[tokio::test]
async fn setup_timeout_passes_through_immediate_success() {
    let result = with_timeout(Duration::from_millis(10), async { Ok(7u32) }).await;
    assert_eq!(result.expect("immediate success should pass through"), 7);
}
