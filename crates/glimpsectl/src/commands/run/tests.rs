use super::*;

#[test]
fn changed_file_names_the_saved_file_after_an_atomic_save() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("index.tsx"), "").unwrap();
    let paths = [
        dir.path().join("sed34lJYP"),
        dir.path().join("index.tsx"),
        dir.path().join("sed34lJYP"),
    ];
    assert_eq!(
        changed_file(&paths, dir.path()).as_deref(),
        Some("restarting: index.tsx")
    );
    assert_eq!(
        changed_file(&paths[..1], dir.path()).as_deref(),
        Some("restarting: sed34lJYP")
    );
    assert_eq!(changed_file(&[], dir.path()), None);
}
