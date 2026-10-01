use super::*;

#[test]
fn a_taken_name_gets_the_next_suffix_and_the_sessions_own_file_is_overwritten() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let name = "Screenshot from 2026-09-24 14-03-11.png";

    let first = Disk
        .write_new(dir.path(), name, None, b"one")
        .expect("first");
    let second = Disk
        .write_new(dir.path(), name, None, b"two")
        .expect("second");
    let again = Disk
        .write_new(dir.path(), name, Some(&second), b"three")
        .expect("own");

    assert_eq!(first, dir.path().join(name));
    assert_eq!(
        second,
        dir.path()
            .join("Screenshot from 2026-09-24 14-03-11 (2).png")
    );
    assert_eq!(again, second);
    assert_eq!(std::fs::read(&first).expect("reads"), b"one");
    assert_eq!(std::fs::read(&second).expect("reads"), b"three");
}
