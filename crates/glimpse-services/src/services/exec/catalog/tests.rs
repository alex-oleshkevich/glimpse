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
fn resolve_file_treats_an_empty_icon_as_absent_and_runs_beside_the_desktop_file() {
    let (_dir, path) = desktop(
        "[Desktop Entry]\nType=Application\nName=Pomodoro\nIcon=\nPath=\nExec=glimpse-applet %i\nImplements=me.aresa.Glimpse.Applet1;\n",
    );
    let entry = DesktopCatalog.resolve_file(&path).expect("entry");
    assert_eq!(entry.icon, None);
    assert_eq!(entry.cwd.as_deref(), path.canonicalize().unwrap().parent());
    assert_eq!(entry.argv.len(), 1);
}
