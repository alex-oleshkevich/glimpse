use super::*;

fn manager(root: &Path) -> PackageManager {
    PackageManager::with_dirs(root.join("applets"), root.join("applications"))
}

fn fixture(root: &Path) -> PathBuf {
    let dir = root.join("source");
    fs::create_dir_all(dir.join("assets")).unwrap();
    fs::write(
        dir.join("org.example.Test.desktop"),
        "[Desktop Entry]\nType=Application\nName=Test\nExec=/bin/true\nImplements=me.aresa.Glimpse.Applet1;\n",
    )
    .unwrap();
    fs::write(dir.join("applet.py"), "print('hi')\n").unwrap();
    fs::write(dir.join("assets/note.txt"), "note\n").unwrap();
    fs::write(dir.join("devconfig.toml"), "").unwrap();
    dir
}

#[test]
fn package_install_and_uninstall_round_trip() {
    let root = tempfile::tempdir().unwrap();
    let dir = fixture(root.path());
    let manager = manager(root.path());
    let out = root.path().join("org.example.Test.glimpse.zip");
    let (metadata, path) = manager.package(&dir, Some(&out)).unwrap();
    assert_eq!(path, out);
    assert_eq!(metadata.id, "org.example.Test");
    let listed: Vec<&str> = metadata
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    assert_eq!(
        listed,
        ["applet.py", "assets/note.txt", "org.example.Test.desktop"]
    );

    let installed = manager.install(&out).unwrap();
    assert_eq!(installed.id, "org.example.Test");
    assert!(installed.dir.join("applet.py").is_file());
    assert!(installed.dir.join("assets/note.txt").is_file());
    assert!(installed.dir.join(METADATA_FILE).is_file());
    assert!(!installed.dir.join("devconfig.toml").exists());
    let applications = root.path().join("applications/org.example.Test.desktop");
    let (contents, _) = marked(&applications).unwrap();
    let desktop = glib::KeyFile::new();
    desktop
        .load_from_data(&contents, glib::KeyFileFlags::NONE)
        .unwrap();
    assert_eq!(
        desktop.string(GROUP, "Path").unwrap().as_str(),
        installed.dir.to_str().unwrap()
    );
    assert_eq!(manager.metadata("org.example.Test").unwrap(), metadata);

    let removed = manager.uninstall("org.example.Test").unwrap();
    assert!(removed.leftovers.is_empty());
    assert!(!installed.dir.exists());
    assert!(!applications.exists());
}

#[test]
fn install_from_directory_replaces_the_previous_install() {
    let root = tempfile::tempdir().unwrap();
    let dir = fixture(root.path());
    let manager = manager(root.path());
    manager.install(&dir).unwrap();
    fs::write(dir.join("applet.py"), "print('changed')\n").unwrap();
    let installed = manager.install(&dir).unwrap();
    assert_eq!(
        fs::read_to_string(installed.dir.join("applet.py")).unwrap(),
        "print('changed')\n"
    );
}

#[test]
fn install_refuses_a_package_whose_contents_do_not_match_the_metadata() {
    let root = tempfile::tempdir().unwrap();
    let dir = fixture(root.path());
    let manager = manager(root.path());
    let out = root.path().join("good.zip");
    manager.package(&dir, Some(&out)).unwrap();
    let tampered = root.path().join("tampered.zip");
    let mut source = zip::ZipArchive::new(File::open(&out).unwrap()).unwrap();
    let mut archive = zip::ZipWriter::new(File::create(&tampered).unwrap());
    for index in 0..source.len() {
        let mut entry = source.by_index(index).unwrap();
        let name = entry.name().unwrap().into_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        if name == "applet.py" {
            bytes = b"print('ho')\n".to_vec();
        }
        archive
            .start_file(&name, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(&bytes).unwrap();
    }
    archive.finish().unwrap();
    let error = manager.install(&tampered).unwrap_err();
    assert!(error.to_string().contains("sha256"), "{error:#}");
    assert!(!root.path().join("applets/org.example.Test").exists());
}

#[test]
fn uninstall_keeps_unlisted_files_and_refuses_unmarked_desktop_entries() {
    let root = tempfile::tempdir().unwrap();
    let dir = fixture(root.path());
    let manager = manager(root.path());
    let installed = manager.install(&dir).unwrap();
    fs::write(installed.dir.join("mine.txt"), "keep\n").unwrap();
    let removed = manager.uninstall("org.example.Test").unwrap();
    assert_eq!(removed.leftovers, [installed.dir.join("mine.txt")]);
    assert!(installed.dir.join("mine.txt").is_file());
    assert!(!installed.dir.join("applet.py").exists());

    let applications = root.path().join("applications");
    fs::write(
        applications.join("org.example.Foreign.desktop"),
        "[Desktop Entry]\nName=Foreign\n",
    )
    .unwrap();
    assert!(manager.uninstall("org.example.Foreign").is_err());
    assert!(applications.join("org.example.Foreign.desktop").is_file());
    assert!(manager.uninstall("../escape").is_err());
}

#[test]
fn package_refuses_a_reserved_metadata_file_and_symlinks() {
    let root = tempfile::tempdir().unwrap();
    let dir = fixture(root.path());
    let manager = manager(root.path());
    fs::write(dir.join(METADATA_FILE), "id = \"x\"\n").unwrap();
    assert!(
        manager
            .package(&dir, Some(&root.path().join("x.zip")))
            .is_err()
    );
    fs::remove_file(dir.join(METADATA_FILE)).unwrap();
    std::os::unix::fs::symlink("applet.py", dir.join("link.py")).unwrap();
    assert!(
        manager
            .package(&dir, Some(&root.path().join("x.zip")))
            .is_err()
    );
}
