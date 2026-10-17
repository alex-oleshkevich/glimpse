use std::path::PathBuf;

use super::*;

fn source(root: &Path) -> PathBuf {
    let dir = root.join("applet \\ source");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("org.example.Test.desktop"), "[Desktop Entry]\nType=Application\nName=Test\nExec=/bin/true\nImplements=me.aresa.Glimpse.Applet1;\n").unwrap();
    fs::write(dir.join("devconfig.toml"), "").unwrap();
    dir
}

#[test]
fn missing_settings_are_empty_but_existing_invalid_settings_are_refused() {
    let root = tempfile::tempdir().unwrap();
    let dir = source(root.path());
    let path = dir.join("devconfig.toml");
    fs::remove_file(&path).unwrap();
    let defaults = settings(&dir, "org.example.Test").unwrap();
    assert_eq!(defaults.applet, "org.example.Test");
    assert!(defaults.env.is_empty() && defaults.options.is_empty());
    for invalid in [
        "broken = [",
        "unknown = true",
        "applet = 'org.example.Other'",
        "env = { NOTIFY_SOCKET = 'x' }",
    ] {
        fs::write(&path, invalid).unwrap();
        assert!(settings(&dir, "org.example.Test").is_err(), "{invalid}");
    }
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(dir.join("missing.toml"), &path).unwrap();
    assert!(settings(&dir, "org.example.Test").is_err());
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
