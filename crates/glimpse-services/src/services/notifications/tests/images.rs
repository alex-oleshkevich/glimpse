use std::time::Duration;

use super::*;

#[test]
fn image_hints_remain_content_images() {
    let app_image = record(
        1,
        Incoming {
            app_id: "org.example.Chat".to_owned(),
            icon: Some("org.example.Chat".to_owned()),
            image: Some("file:///tmp/org.example.Chat.png".to_owned()),
            ..incoming("Chat", "Marta")
        },
    );
    assert_eq!(
        app_image.image.as_deref(),
        Some("/tmp/org.example.Chat.png")
    );

    let screenshot = record(
        2,
        Incoming {
            image: Some("/tmp/Screenshot_2026-09-12.png".to_owned()),
            ..incoming("Screenshots", "Screenshot captured")
        },
    );
    assert_eq!(
        screenshot.image.as_deref(),
        Some("/tmp/Screenshot_2026-09-12.png")
    );
}

#[test]
fn icons_and_images_resolve_from_names_paths_and_uris() {
    let owned = |value: &str| Some(value.to_owned());
    for (app_icon, image_path, image_data, expected) in [
        (
            "dialog-warning",
            None,
            None,
            (owned("dialog-warning"), None),
        ),
        ("file:///a%20b.png", None, None, (owned("/a b.png"), None)),
        ("/x.png", None, None, (owned("/x.png"), None)),
        (
            "",
            Some("file:///a%20b.png"),
            None,
            (None, owned("/a b.png")),
        ),
        ("app", Some("/y.png"), None, (owned("app"), owned("/y.png"))),
        (
            "",
            Some("dialog-warning"),
            None,
            (owned("dialog-warning"), None),
        ),
        (
            "file:///a.png",
            Some("/b.png"),
            None,
            (owned("/a.png"), owned("/b.png")),
        ),
        (
            "/a.png",
            Some("/b.png"),
            owned("/c.png"),
            (owned("/a.png"), owned("/c.png")),
        ),
        ("http://x/y.png", Some("rel/z.png"), None, (None, None)),
        ("app", Some("audio-volume-high"), None, (owned("app"), None)),
    ] {
        assert_eq!(
            visuals(app_icon, image_path, image_data),
            expected,
            "{app_icon} {image_path:?}"
        );
    }
}

#[test]
fn image_data_encodes_as_a_png_of_the_same_size() {
    let padded = [[255, 0, 0, 255, 0, 255, 0, 255, 9, 9], [0; 10]].concat();
    let pixels = pixels(&image_data((2, 2, 10), true, 8, 4, padded)).expect("valid pixels");
    let bytes = encode(&pixels).expect("encoded");

    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let reader = decoder.read_info().expect("a png");
    let info = reader.info();
    assert_eq!((info.width, info.height), (2, 2));
    assert_eq!(info.color_type, png::ColorType::Rgba);
}

#[test]
fn malformed_image_data_is_ignored() {
    for hints in [
        image_data((2, 2, 8), true, 16, 4, vec![0; 32]),
        image_data((2, 2, 8), true, 8, 3, vec![0; 32]),
        image_data((2, 2, 8), true, 8, 4, vec![0; 15]),
        image_data((2, 2, 4), true, 8, 4, vec![0; 32]),
        image_data((5000, 1, 20000), true, 8, 4, vec![0; 20000]),
        image_data((0, 1, 4), true, 8, 4, vec![0; 4]),
    ] {
        assert!(pixels(&hints).is_none());
    }
}

#[test]
fn only_a_file_directly_in_the_images_directory_is_ever_deleted() {
    let dir = Path::new("/fixture/runtime/glimpse/notifications/7");
    assert!(generated(
        "/fixture/runtime/glimpse/notifications/7/3-1.png",
        dir
    ));
    assert!(!generated(
        "/fixture/runtime/glimpse/notifications/7/../../../../home/u/.ssh/id_ed25519",
        dir
    ));
    assert!(!generated("/fixture/runtime/glimpse/notifications/7", dir));
    assert!(!generated("/home/u/picture.png", dir));
}

fn isolated_image_test(name: &str) -> bool {
    if std::env::var("GLIMPSE_IMAGE_TEST").as_deref() == Ok(name) {
        return false;
    }
    let scratch = tempfile::tempdir().expect("a scratch directory");
    assert!(
        std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args(["--exact", name])
            .env("GLIMPSE_IMAGE_TEST", name)
            .env("XDG_RUNTIME_DIR", scratch.path())
            .status()
            .expect("isolated test runs")
            .success()
    );
    true
}

async fn dead_pid() -> u32 {
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("a helper process");
    let pid = child.id();
    child.wait().expect("the helper exits");
    pid
}

#[tokio::test]
async fn a_dead_pids_images_are_swept_and_a_live_pids_survive() {
    if isolated_image_test(
        concat!(
            module_path!(),
            "::a_dead_pids_images_are_swept_and_a_live_pids_survive"
        )
        .trim_start_matches("glimpse_services::"),
    ) {
        return;
    }
    let root = images_dir()
        .expect("the image directory")
        .parent()
        .expect("the root")
        .to_owned();
    let dead = root.join(dead_pid().await.to_string());
    let live = root.join(std::process::id().to_string());
    tokio::fs::create_dir_all(&dead)
        .await
        .expect("the dead directory is created");
    tokio::fs::create_dir_all(&live)
        .await
        .expect("the live directory is created");
    tokio::fs::write(dead.join("x.png"), [0u8; 8])
        .await
        .expect("the dead png is written");
    tokio::fs::write(live.join("y.png"), [0u8; 8])
        .await
        .expect("the live png is written");

    let (handle, _sender, _state, cancel, running) = dnd_service().await;
    tokio::time::timeout(
        Duration::from_secs(5),
        handle
            .health()
            .wait_for(|state| !matches!(state, crate::ServiceState::Starting)),
    )
    .await
    .expect("startup finishes")
    .expect("the service is alive");

    assert!(!dead.exists(), "a dead pid's images are swept");
    assert!(live.exists(), "a live pid's images survive the sweep");
    cancel.cancel();
    running.await.expect("joined");
}

#[tokio::test]
async fn a_sweep_failure_is_skipped_and_the_sweep_continues() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let root = scratch.path().join("notifications");
    let stuck = root.join(dead_pid().await.to_string());
    let doomed = root.join(dead_pid().await.to_string());
    tokio::fs::create_dir_all(&root)
        .await
        .expect("the root is created");
    tokio::fs::write(&stuck, b"not a directory")
        .await
        .expect("the stuck entry is written");
    tokio::fs::create_dir_all(&doomed)
        .await
        .expect("the doomed directory is created");

    clear_stale_images(&root).await;

    assert!(stuck.exists(), "an unremovable entry is left alone");
    assert!(!doomed.exists(), "the sweep continues past a failure");
}
