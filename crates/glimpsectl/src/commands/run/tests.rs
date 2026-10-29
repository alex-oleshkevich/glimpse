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

#[test]
fn locate_accepts_a_built_applet_whose_binary_does_not_exist_yet() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("justfile"), "build:\n").unwrap();
    let desktop = dir.path().join("me.test.Built.desktop");
    fs::write(
        &desktop,
        "[Desktop Entry]\nType=Application\nName=Built\nExec=./missing-binary\nImplements=me.aresa.Glimpse.Applet1;\n",
    )
    .unwrap();
    assert_eq!(
        locate(dir.path()).unwrap(),
        ("me.test.Built".to_owned(), desktop)
    );
}
