use super::*;

#[test]
fn a_zero_refcount_is_read_from_the_third_field() {
    let modules = "uvcvideo 106496 0 - Live 0x0000000000000000\n";
    assert_eq!(uvcvideo_refcount(modules), Some(0));
}

#[test]
fn a_nonzero_refcount_is_read_from_the_third_field() {
    let modules = "other_mod 16384 1 - Live 0x0\nuvcvideo 106496 2 - Live 0x0\n";
    assert_eq!(uvcvideo_refcount(modules), Some(2));
}

#[test]
fn a_missing_uvcvideo_line_is_none() {
    assert_eq!(
        uvcvideo_refcount("other_mod 16384 1 - Live 0x0\n"),
        None,
        "absent is not the same fact as present-and-idle: only the former means this \
             hardware cannot be observed at all"
    );
}

#[test]
fn a_zero_refcount_skips_the_walk_entirely() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    fake_modules(dir.path(), 0);
    fake_proc(dir.path(), 4242, "ffmpeg", Some("/dev/video0"));

    assert!(
        scan(dir.path())
            .expect("uvcvideo is loaded in this fixture")
            .is_empty(),
        "a zero uvcvideo refcount must skip the /proc walk entirely, not merely find nothing \
             in it: this fabricated process would otherwise be found and named"
    );
}

#[test]
fn a_process_holding_the_camera_is_named() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    fake_modules(dir.path(), 1);
    fake_proc(dir.path(), 4242, "ffmpeg", Some("/dev/video0"));

    let facts = scan(dir.path()).expect("uvcvideo is loaded in this fixture");
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].app.as_deref(), Some("ffmpeg"));
}

#[test]
fn pipewire_and_wireplumber_are_the_daemon_not_the_app() {
    assert_eq!(app_from_comm("pipewire"), None);
    assert_eq!(app_from_comm("wireplumber"), None);
    assert_eq!(app_from_comm("chrome"), Some("chrome".to_owned()));
}

#[test]
fn a_process_with_no_camera_fd_is_ignored() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    fake_modules(dir.path(), 1);
    fake_proc(dir.path(), 4242, "sshd", None);

    assert!(
        scan(dir.path())
            .expect("uvcvideo is loaded in this fixture")
            .is_empty()
    );
}

#[test]
fn an_unreadable_modules_file_is_reported_unavailable() {
    let dir = tempfile::tempdir().expect("a temporary directory");

    assert_eq!(scan(dir.path()), Err("camera: /proc/modules is unreadable"));
}

#[test]
fn an_absent_uvcvideo_module_is_reported_unavailable_rather_than_idle() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(dir.path().join("modules"), "other_mod 16384 1 - Live 0x0\n")
        .expect("write fake /proc/modules");

    assert_eq!(
        scan(dir.path()),
        Err(
            "camera: uvcvideo is not loaded, so camera activity cannot be observed on this \
                 hardware"
        )
    );
}
