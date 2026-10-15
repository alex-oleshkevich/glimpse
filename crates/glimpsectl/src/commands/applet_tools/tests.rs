use super::*;

fn source(root: &Path) -> PathBuf {
    let dir = root.join("applet \\ source");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("org.example.Test.desktop"), "[Desktop Entry]\nType=Application\nName=Test\nExec=/bin/true\nImplements=me.aresa.Glimpse.Applet1;\n").unwrap();
    fs::write(dir.join("config.toml"), "").unwrap();
    dir
}

#[test]
fn installation_round_trips_escaped_paths_and_refuses_other_files() {
    let root = tempfile::tempdir().unwrap();
    let dir = source(root.path());
    let applications = root.path().join("applications");
    let id = install(&dir, &applications).unwrap();
    let target = applications.join(format!("{id}.desktop"));
    let (contents, _) = marked(&target).unwrap();
    let desktop = glib::KeyFile::new();
    desktop
        .load_from_data(&contents, glib::KeyFileFlags::NONE)
        .unwrap();
    assert_eq!(
        desktop.string(GROUP, "Path").unwrap().as_str(),
        dir.to_str().unwrap()
    );
    assert!(install(&dir, &applications).is_err());
    uninstall(&id, &applications).unwrap();
    assert!(!target.exists());
    fs::write(&target, "[Desktop Entry]\nName=Unrelated\n").unwrap();
    assert!(uninstall(&id, &applications).is_err());
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "[Desktop Entry]\nName=Unrelated\n"
    );
    assert!(install(&dir, &applications).is_err());
    fs::remove_file(&target).unwrap();
    let unrelated = root.path().join("unrelated");
    fs::write(&unrelated, contents).unwrap();
    std::os::unix::fs::symlink(&unrelated, &target).unwrap();
    assert!(uninstall(&id, &applications).is_err());
    assert!(install(&dir, &applications).is_err());
    assert!(unrelated.exists());
    assert!(uninstall("../escape", &applications).is_err());
}

#[tokio::test]
async fn bounded_wire_reader_rejects_oversized_and_invalid_messages() {
    let mut reader = BufReader::new(&b"{\"t\":\"hello\",\"v\":1}\n"[..]);
    assert!(matches!(
        message(&mut reader).await.unwrap(),
        FromApplet::Hello { v: 1 }
    ));
    let large = vec![b'x'; MAX_LINE + 2];
    assert!(
        message(&mut BufReader::new(large.as_slice()))
            .await
            .unwrap_err()
            .to_string()
            .contains("exceeds")
    );
    assert!(
        message(&mut BufReader::new(&b"garbage\n"[..]))
            .await
            .is_err()
    );
    assert!(message(&mut BufReader::new(&b""[..])).await.is_err());
}
