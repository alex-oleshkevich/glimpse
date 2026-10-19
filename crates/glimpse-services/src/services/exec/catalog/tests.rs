use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::*;

fn expanded(exec: &str, icon: Option<&str>) -> Vec<String> {
    let path = Path::new("/usr/share/applications/me.example.Pomodoro.desktop");
    expand(exec, "Pomodoro", icon, path).expect("expanded")
}

#[test]
fn expand_table() {
    let path = Path::new("/usr/share/applications/me.example.Pomodoro.desktop");
    assert_eq!(expanded("a %f b", None), ["a", "b"]);
    for code in ["%F", "%u", "%U", "%d", "%D", "%n", "%N", "%v", "%m"] {
        assert_eq!(expanded(&format!("a {code} b"), None), ["a", "b"]);
    }
    assert_eq!(expanded("a %i", Some("x")), ["a", "--icon", "x"]);
    assert_eq!(expanded("a %i", None), ["a"]);
    assert_eq!(expanded("a %c", None), ["a", "Pomodoro"]);
    let displayed = path.display().to_string();
    assert_eq!(expanded("a %k", None), ["a", displayed.as_str()]);
    assert_eq!(expanded("a %%", None), ["a", "%"]);
    assert_eq!(expanded("a --n=%c", None), ["a", "--n=Pomodoro"]);
    assert_eq!(expanded("a +%%H", None), ["a", "+%H"]);
    assert_eq!(expanded("a x%fy", None), ["a", "xy"]);
    assert_eq!(
        expand("a %z", "Pomodoro", None, path).expect_err("unknown code"),
        "unknown field code %z"
    );
    assert_eq!(
        expand("a -x=%z", "Pomodoro", None, path).expect_err("embedded unknown code"),
        "unknown field code %z"
    );
    assert_eq!(
        expand("a b%", "Pomodoro", None, path).expect_err("trailing percent"),
        "trailing % in Exec"
    );
    for code in ["%F", "%U", "%i"] {
        assert_eq!(
            expand(&format!("a x{code}"), "Pomodoro", None, path)
                .expect_err("embedded unsupported code"),
            format!("unknown field code {code}")
        );
    }
    assert!(expand("", "Pomodoro", None, path).is_err());
    assert!(expand("%f", "Pomodoro", None, path).is_err());
    assert_eq!(expanded(r#""a b" c"#, None), ["a b", "c"]);
}

fn desktop(body: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let program = dir.path().join("glimpse-applet");
    std::fs::write(&program, "#!/bin/sh\nexit 0\n").expect("stub script");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
        .expect("executable stub");
    let path = dir.path().join("applet.desktop");
    std::fs::write(
        &path,
        body.replace("glimpse-applet", &program.to_string_lossy()),
    )
    .expect("desktop file");
    (dir, path)
}

fn refusal(body: &str) -> (PathBuf, String) {
    let (_dir, path) = desktop(body);
    let error = DesktopCatalog
        .resolve_file(&path)
        .expect_err("desktop entry refused");
    (path, error)
}

#[test]
fn resolve_file_refuses_hidden() {
    let (path, error) = refusal(
        "[Desktop Entry]\nType=Application\nName=Pomodoro\nExec=glimpse-applet\nHidden=true\nImplements=me.aresa.Glimpse.Applet1;\n",
    );
    assert!(error.contains("Hidden"), "{error}");
    assert!(error.contains(&path.display().to_string()), "{error}");
}

#[test]
fn resolve_file_refuses_a_missing_interface() {
    let (path, error) =
        refusal("[Desktop Entry]\nType=Application\nName=Pomodoro\nExec=glimpse-applet\n");
    assert!(error.contains("Implements"), "{error}");
    assert!(error.contains(INTERFACE), "{error}");
    assert!(error.contains(&path.display().to_string()), "{error}");
}

#[test]
fn resolve_file_refuses_a_different_interface() {
    let (path, error) = refusal(
        "[Desktop Entry]\nType=Application\nName=Pomodoro\nExec=glimpse-applet\nImplements=org.example.Other;\n",
    );
    assert!(error.contains("Implements"), "{error}");
    assert!(error.contains(INTERFACE), "{error}");
    assert!(error.contains(&path.display().to_string()), "{error}");
}

#[test]
fn resolve_file_refuses_dbus_activatable() {
    let (path, error) = refusal(
        "[Desktop Entry]\nType=Application\nName=Pomodoro\nExec=glimpse-applet\nDBusActivatable=true\nImplements=me.aresa.Glimpse.Applet1;\n",
    );
    assert!(error.contains("DBusActivatable"), "{error}");
    assert!(error.contains(&path.display().to_string()), "{error}");
}

#[test]
fn resolve_file_refuses_terminal() {
    let (path, error) = refusal(
        "[Desktop Entry]\nType=Application\nName=Pomodoro\nExec=glimpse-applet\nTerminal=true\nImplements=me.aresa.Glimpse.Applet1;\n",
    );
    assert!(error.contains("Terminal"), "{error}");
    assert!(error.contains(&path.display().to_string()), "{error}");
}

#[test]
fn resolve_file_refuses_an_unknown_field_code() {
    let (path, error) = refusal(
        "[Desktop Entry]\nType=Application\nName=Pomodoro\nExec=glimpse-applet %z\nImplements=me.aresa.Glimpse.Applet1;\n",
    );
    assert!(error.contains("unknown field code %z"), "{error}");
    assert!(error.contains(&path.display().to_string()), "{error}");
}

#[test]
fn resolve_file_refuses_a_missing_exec() {
    let (path, error) = refusal(
        "[Desktop Entry]\nType=Application\nName=Pomodoro\nImplements=me.aresa.Glimpse.Applet1;\n",
    );
    assert!(error.contains("Exec is missing"), "{error}");
    assert!(error.contains(&path.display().to_string()), "{error}");
}

#[test]
fn resolve_file_refuses_a_failed_try_exec() {
    let (path, error) = refusal(
        "[Desktop Entry]\nType=Application\nName=Pomodoro\nExec=glimpse-applet\nTryExec=/nonexistent/x\nImplements=me.aresa.Glimpse.Applet1;\n",
    );
    assert!(error.contains("cannot be loaded"), "{error}");
    assert!(error.contains(&path.display().to_string()), "{error}");
}

#[test]
fn resolve_file_refuses_a_missing_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("missing.desktop");
    let error = DesktopCatalog
        .resolve_file(&path)
        .expect_err("missing file");
    assert!(error.contains("no desktop entry"), "{error}");
    assert!(error.contains(&path.display().to_string()), "{error}");
}

#[test]
fn resolve_file_returns_the_entry() {
    let (dir, path) = desktop(
        "[Desktop Entry]\nType=Application\nName=Pomodoro\nIcon=me.example.Pomodoro\nExec=glimpse-applet %k %c\nPath=/tmp/work\nNoDisplay=true\nImplements=me.aresa.Glimpse.Applet1;\n",
    );
    let program = dir.path().join("glimpse-applet").display().to_string();
    let entry = DesktopCatalog.resolve_file(&path).expect("entry");
    assert_eq!(
        entry.argv,
        [
            program.clone(),
            path.display().to_string(),
            "Pomodoro".to_owned()
        ]
    );
    assert_eq!(entry.cwd, Some(PathBuf::from("/tmp/work")));
    assert_eq!(entry.name, "Pomodoro");
    assert_eq!(entry.icon.as_deref(), Some("me.example.Pomodoro"));
    assert_eq!(entry.path, path);
    assert_eq!(entry.exec, format!("{program} %k %c"));
}

#[test]
fn resolve_file_absolutizes_a_relative_exec() {
    let dir = tempfile::tempdir().expect("temp dir");
    let program = dir.path().join("glimpse-applet");
    std::fs::write(&program, "#!/bin/sh\nexit 0\n").expect("stub script");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
        .expect("executable stub");
    let path = dir.path().join("applet.desktop");
    std::fs::write(
        &path,
        "[Desktop Entry]\nType=Application\nName=Pomodoro\nExec=./glimpse-applet --flag\nImplements=me.aresa.Glimpse.Applet1;\n",
    )
    .expect("desktop file");
    let entry = DesktopCatalog.resolve_file(&path).expect("entry");
    let expected = dir
        .path()
        .canonicalize()
        .unwrap()
        .join("glimpse-applet")
        .display()
        .to_string();
    assert_eq!(entry.argv, [expected, "--flag".to_owned()]);
    assert_eq!(entry.path, path);
}

#[test]
fn resolve_file_treats_an_empty_icon_as_absent_and_runs_beside_the_desktop_file() {
    let (_dir, path) = desktop(
        "[Desktop Entry]\nType=Application\nName=Pomodoro\nIcon=\nPath=\nExec=glimpse-applet %i\nImplements=me.aresa.Glimpse.Applet1;\n",
    );
    let entry = DesktopCatalog.resolve_file(&path).expect("entry");
    assert_eq!(entry.icon, None);
    assert_eq!(entry.cwd.as_deref(), path.canonicalize().unwrap().parent());
    assert_eq!(entry.argv.len(), 1);
}

#[test]
fn resolve_searches_directories_in_order_without_caching_entries() {
    let dir = tempfile::tempdir().expect("data directories");
    let user = dir.path().join("user");
    let system = dir.path().join("system");
    std::fs::create_dir_all(&user).expect("user applications");
    std::fs::create_dir_all(&system).expect("system applications");
    let user_entry = user.join("me.example.Test.desktop");
    let system_entry = system.join("me.example.Test.desktop");
    let body = "[Desktop Entry]\nType=Application\nName=Test applet\nExec=/bin/true\nImplements=me.aresa.Glimpse.Applet1;\n";
    std::fs::write(&system_entry, body).expect("system entry");
    let dirs = [user, system];
    assert_eq!(
        DesktopCatalog
            .resolve_in("me.example.Test", &dirs)
            .expect("system entry")
            .path,
        system_entry
    );
    std::fs::write(&user_entry, body).expect("user entry");
    assert_eq!(
        DesktopCatalog
            .resolve_in("me.example.Test", &dirs)
            .expect("user override")
            .path,
        user_entry
    );
    std::fs::write(
        &user_entry,
        body.replace("Name=Test applet", "Name=Replacement"),
    )
    .expect("replace entry");
    assert_eq!(
        DesktopCatalog
            .resolve_in("me.example.Test", &dirs)
            .expect("replacement")
            .name,
        "Replacement"
    );
    std::fs::remove_file(&user_entry).expect("remove user entry");
    assert_eq!(
        DesktopCatalog
            .resolve_in("me.example.Test", &dirs)
            .expect("system fallback")
            .path,
        system_entry
    );
    std::fs::remove_file(&system_entry).expect("remove system entry");
    assert_eq!(
        DesktopCatalog
            .resolve_in("me.example.Test", &dirs)
            .expect_err("missing entry"),
        "me.example.Test: not installed, or not Type=Application, or its TryExec/Exec program is not on PATH"
    );
}

#[test]
fn resolve_does_not_skip_an_invalid_higher_priority_entry() {
    let dir = tempfile::tempdir().expect("data directories");
    let user = dir.path().join("user");
    let system = dir.path().join("system");
    std::fs::create_dir_all(&user).expect("user applications");
    std::fs::create_dir_all(&system).expect("system applications");
    let path = user.join("me.example.Test.desktop");
    let body = "[Desktop Entry]\nType=Application\nName=Test applet\nExec=/bin/true\nImplements=me.aresa.Glimpse.Applet1;\n";
    std::fs::write(system.join("me.example.Test.desktop"), body).expect("system entry");
    let dirs = [user, system];
    for refused in [
        format!("{body}Hidden=true\n"),
        format!("{body}TryExec=/nonexistent/x\n"),
        body.replace("Type=Application", "Type=Link"),
    ] {
        std::fs::write(&path, refused).expect("invalid user entry");
        assert!(DesktopCatalog.resolve_in("me.example.Test", &dirs).is_err());
    }
}
