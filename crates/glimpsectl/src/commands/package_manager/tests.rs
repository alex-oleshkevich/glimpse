use super::*;

fn manager(root: &Path) -> PackageManager {
    PackageManager::with_dirs(root.join("applets"), root.join("applications"))
}

const DESKTOP: &str = "[Desktop Entry]\nType=Application\nName=Test\nExec=/bin/true\nImplements=me.aresa.Glimpse.Applet1;\n";

fn fixture(root: &Path) -> PathBuf {
    let dir = root.join("source");
    fs::create_dir_all(dir.join("assets")).unwrap();
    fs::write(dir.join("org.example.Test.desktop"), DESKTOP).unwrap();
    fs::write(dir.join("applet.py"), "print('hi')\n").unwrap();
    fs::write(dir.join("assets/note.txt"), "note\n").unwrap();
    fs::write(dir.join("devconfig.toml"), "").unwrap();
    fs::write(
        dir.join("justfile"),
        "build:\n    mkdir -p dist/assets\n    cp applet.py org.example.Test.desktop dist/\n    cp assets/note.txt dist/assets/\n",
    )
    .unwrap();
    dir
}

fn make_zip(path: &Path, id: &str, entries: &[(&str, &[u8], u32)]) {
    let mut metadata = format!("id = \"{id}\"\n");
    for (name, bytes, _) in entries {
        metadata.push_str(&format!(
            "\n[[files]]\npath = \"{name}\"\nsha256 = \"{}\"\n",
            sha256_hex(bytes)
        ));
    }
    let mut archive = zip::ZipWriter::new(File::create(path).unwrap());
    for (name, bytes, mode) in entries {
        let options = zip::write::SimpleFileOptions::default().unix_permissions(*mode);
        if mode & 0o170000 == 0o120000 {
            archive
                .add_symlink(
                    name.to_string(),
                    String::from_utf8(bytes.to_vec()).unwrap(),
                    options,
                )
                .unwrap();
        } else {
            archive.start_file(name.to_string(), options).unwrap();
            archive.write_all(bytes).unwrap();
        }
    }
    archive
        .start_file(
            METADATA_FILE.to_owned(),
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    archive.write_all(metadata.as_bytes()).unwrap();
    archive.finish().unwrap();
}

#[test]
fn package_json_keeps_build_output_on_stderr() {
    if std::env::var_os("GLIMPSE_TEST_PACKAGE_JSON_CHILD").is_some() {
        let root = tempfile::tempdir().unwrap();
        let dir = fixture(root.path());
        let justfile = dir.join("justfile");
        let recipe = fs::read_to_string(&justfile).unwrap();
        fs::write(
            &justfile,
            format!("{recipe}    @echo package-build-stdout\n"),
        )
        .unwrap();
        applet_package(
            PackageArgs {
                dir,
                out: Some(root.path().join("test.glimpse.zip")),
                install: false,
            },
            true,
        )
        .unwrap();
        return;
    }

    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "commands::package_manager::tests::package_json_keeps_build_output_on_stderr",
            "--nocapture",
        ])
        .env("GLIMPSE_TEST_PACKAGE_JSON_CHILD", "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("\"id\""), "{stdout}");
    assert!(!stdout.contains("package-build-stdout"), "{stdout}");
    assert!(stderr.contains("package-build-stdout"), "{stderr}");
}

#[test]
fn package_out_directory_receives_the_named_archive() {
    let root = tempfile::tempdir().unwrap();
    let dir = fixture(root.path());
    let out_dir = root.path().join("out");
    fs::create_dir(&out_dir).unwrap();
    let (_, path) = manager(root.path()).package(&dir, Some(&out_dir)).unwrap();
    assert_eq!(path, out_dir.join("org.example.Test.glimpse.zip"));
    assert!(path.is_file());
}

#[test]
fn package_install_and_uninstall_round_trip() {
    let root = tempfile::tempdir().unwrap();
    let dir = fixture(root.path());
    let manager = manager(root.path());
    let out = root.path().join("org.example.Test.glimpse.zip");
    let (metadata, path) = manager.package(&dir, Some(&out)).unwrap();
    assert_eq!(path, out);
    assert_eq!(fs::metadata(&out).unwrap().mode() & 0o777, 0o644);
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
    assert!(!installed.dir.join("justfile").exists());
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
fn upgrade_refused_by_an_unowned_file_keeps_the_previous_install() {
    let root = tempfile::tempdir().unwrap();
    let dir = fixture(root.path());
    let manager = manager(root.path());
    let installed = manager.install(&dir).unwrap();
    fs::write(installed.dir.join("cache.db"), "mine\n").unwrap();
    fs::write(dir.join("applet.py"), "print('changed')\n").unwrap();
    assert!(manager.install(&dir).is_err());
    assert_eq!(
        fs::read_to_string(installed.dir.join("applet.py")).unwrap(),
        "print('hi')\n"
    );
    assert!(installed.dir.join(METADATA_FILE).is_file());
    assert!(
        root.path()
            .join("applications/org.example.Test.desktop")
            .is_file()
    );
}

#[test]
fn failed_upgrade_restores_the_previous_install() {
    let root = tempfile::tempdir().unwrap();
    let dir = fixture(root.path());
    let manager = manager(root.path());
    let installed = manager.install(&dir).unwrap();
    let hidden = format!("{DESKTOP}Hidden=true\n");
    let bad = root.path().join("bad.zip");
    make_zip(
        &bad,
        "org.example.Test",
        &[
            ("org.example.Test.desktop", hidden.as_bytes(), 0o644),
            ("applet.py", b"print('bad')\n", 0o644),
        ],
    );
    assert!(manager.install(&bad).is_err());
    assert_eq!(
        fs::read_to_string(installed.dir.join("applet.py")).unwrap(),
        "print('hi')\n"
    );
    let applications = root.path().join("applications/org.example.Test.desktop");
    assert!(marked(&applications).is_ok());
}

#[test]
fn install_refuses_a_dangling_symlink_desktop_entry() {
    let root = tempfile::tempdir().unwrap();
    let dir = fixture(root.path());
    let manager = manager(root.path());
    let applications = root.path().join("applications");
    fs::create_dir_all(&applications).unwrap();
    let outside = root.path().join("outside.txt");
    std::os::unix::fs::symlink(&outside, applications.join("org.example.Test.desktop")).unwrap();
    assert!(manager.install(&dir).is_err());
    assert!(!outside.exists());
    assert!(applications.join("org.example.Test.desktop").is_symlink());
}

#[test]
fn install_refuses_an_unmarked_desktop_entry() {
    let root = tempfile::tempdir().unwrap();
    let dir = fixture(root.path());
    let manager = manager(root.path());
    let applications = root.path().join("applications");
    fs::create_dir_all(&applications).unwrap();
    fs::write(
        applications.join("org.example.Test.desktop"),
        "[Desktop Entry]\nName=Foreign\n",
    )
    .unwrap();
    assert!(manager.install(&dir).is_err());
    assert!(!root.path().join("applets/org.example.Test").exists());
    assert_eq!(
        fs::read_to_string(applications.join("org.example.Test.desktop")).unwrap(),
        "[Desktop Entry]\nName=Foreign\n"
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
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(&bytes).unwrap();
    }
    archive.finish().unwrap();
    let error = manager.install(&tampered).unwrap_err();
    assert!(error.to_string().contains("sha256"), "{error:#}");
    assert!(!root.path().join("applets/org.example.Test").exists());
}

#[test]
fn install_refuses_a_symlink_entry() {
    let root = tempfile::tempdir().unwrap();
    let manager = manager(root.path());
    let package = root.path().join("link.zip");
    make_zip(
        &package,
        "org.example.Test",
        &[
            ("org.example.Test.desktop", DESKTOP.as_bytes(), 0o644),
            ("link.py", b"applet.py", 0o120777),
        ],
    );
    assert!(manager.install(&package).is_err());
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
fn uninstall_with_an_unmarked_desktop_entry_deletes_nothing() {
    let root = tempfile::tempdir().unwrap();
    let dir = fixture(root.path());
    let manager = manager(root.path());
    let installed = manager.install(&dir).unwrap();
    let applications = root.path().join("applications/org.example.Test.desktop");
    fs::write(&applications, "[Desktop Entry]\nName=Foreign\n").unwrap();
    assert!(manager.uninstall("org.example.Test").is_err());
    assert!(installed.dir.join("applet.py").is_file());
    assert!(installed.dir.join(METADATA_FILE).is_file());
}

#[test]
fn package_refuses_invalid_settings_and_a_missing_exec_target() {
    let root = tempfile::tempdir().unwrap();
    let dir = fixture(root.path());
    let manager = manager(root.path());
    fs::write(dir.join("devconfig.toml"), "broken = [").unwrap();
    assert!(
        manager
            .package(&dir, Some(&root.path().join("x.zip")))
            .is_err()
    );
    fs::write(dir.join("devconfig.toml"), "").unwrap();
    fs::write(
        dir.join("org.example.Test.desktop"),
        "[Desktop Entry]\nType=Application\nName=Test\nExec=./run.sh\nImplements=me.aresa.Glimpse.Applet1;\n",
    )
    .unwrap();
    assert!(
        manager
            .package(&dir, Some(&root.path().join("x.zip")))
            .is_err()
    );
}

#[test]
fn package_refuses_a_reserved_metadata_file_and_symlinks_in_dist() {
    let root = tempfile::tempdir().unwrap();
    let dir = fixture(root.path());
    let manager = manager(root.path());
    fs::write(dir.join(METADATA_FILE), "id = \"x\"\n").unwrap();
    fs::write(
        dir.join("justfile"),
        "build:\n    mkdir -p dist\n    cp applet.py org.example.Test.desktop metadata.toml dist/\n",
    )
    .unwrap();
    assert!(
        manager
            .package(&dir, Some(&root.path().join("x.zip")))
            .is_err()
    );
    fs::remove_file(dir.join(METADATA_FILE)).unwrap();
    fs::write(
        dir.join("justfile"),
        "build:\n    mkdir -p dist\n    cp org.example.Test.desktop dist/\n    ln -s ../applet.py dist/link.py\n",
    )
    .unwrap();
    assert!(
        manager
            .package(&dir, Some(&root.path().join("x.zip")))
            .is_err()
    );
}
