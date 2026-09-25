use std::path::{Path, PathBuf};

use gio::prelude::AppInfoExt;
use gio_unix::DesktopAppInfo;

pub const INTERFACE: &str = "me.aresa.Glimpse.Applet1";

pub trait Catalog: Send + Sync + 'static {
    fn resolve(&self, id: &str) -> Result<Entry, String>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub argv: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub name: String,
    pub icon: Option<String>,
    pub path: PathBuf,
    pub exec: String,
}

pub struct DesktopCatalog;

impl Catalog for DesktopCatalog {
    fn resolve(&self, id: &str) -> Result<Entry, String> {
        let info = DesktopAppInfo::new(&format!("{id}.desktop"))
            .ok_or_else(|| format!("{id}: not installed, or not Type=Application, or its TryExec/Exec program is not on PATH"))?;
        entry_of(&info, id)
    }
}

impl DesktopCatalog {
    pub fn resolve_file(&self, path: &Path) -> Result<Entry, String> {
        let subject = path.display().to_string();
        let info = DesktopAppInfo::from_filename(path).ok_or_else(|| {
            if path.exists() {
                format!("{subject}: cannot be loaded (not Type=Application, or its TryExec/Exec program is not on PATH)")
            } else {
                format!("{subject}: no desktop entry")
            }
        })?;
        entry_of(&info, &subject)
    }

    pub fn installed(&self) -> Vec<(String, Result<Entry, String>)> {
        let mut rows = Vec::new();
        for info in DesktopAppInfo::implementations(INTERFACE) {
            let Some(raw) = info.id() else {
                continue;
            };
            let id = match raw.strip_suffix(".desktop") {
                Some(id) => id.to_owned(),
                None => raw.to_string(),
            };
            let resolved = entry_of(&info, &id);
            rows.push((id, resolved));
        }
        rows.sort_by(|left, right| left.0.cmp(&right.0));
        rows
    }
}

fn entry_of(info: &DesktopAppInfo, subject: &str) -> Result<Entry, String> {
    if info.is_hidden() {
        return Err(format!("{subject}: Hidden"));
    }
    if !info
        .string_list("Implements")
        .iter()
        .any(|item| item.as_str() == INTERFACE)
    {
        return Err(format!(
            "{subject}: Implements does not include {INTERFACE}"
        ));
    }
    if info.boolean("DBusActivatable") {
        return Err(format!("{subject}: DBusActivatable"));
    }
    if info.boolean("Terminal") {
        return Err(format!("{subject}: Terminal"));
    }
    let exec = info
        .string("Exec")
        .ok_or_else(|| format!("{subject}: Exec is missing"))?
        .to_string();
    let name = info.name().to_string();
    let icon = info
        .string("Icon")
        .filter(|icon| !icon.is_empty())
        .map(|icon| icon.to_string());
    let path = info
        .filename()
        .ok_or_else(|| format!("{subject}: no filename"))?;
    let cwd = info
        .string("Path")
        .filter(|cwd| !cwd.is_empty())
        .map(|cwd| PathBuf::from(cwd.as_str()));
    let argv = expand(&exec, &name, icon.as_deref(), &path)
        .map_err(|error| format!("{subject}: {error}"))?;
    Ok(Entry {
        argv,
        cwd,
        name,
        icon,
        path,
        exec,
    })
}

// ponytail: GIO's own Exec expansion is private.
pub fn expand(
    exec: &str,
    name: &str,
    icon: Option<&str>,
    path: &Path,
) -> Result<Vec<String>, String> {
    let parsed = gio::glib::shell_parse_argv(exec).map_err(|error| error.to_string())?;
    let mut argv = Vec::new();
    for arg in parsed {
        let arg = arg.to_str().ok_or_else(|| "Exec is not utf-8".to_owned())?;
        if matches!(
            arg,
            "%f" | "%F" | "%u" | "%U" | "%d" | "%D" | "%n" | "%N" | "%v" | "%m"
        ) {
            continue;
        }
        if arg == "%i" {
            if let Some(icon) = icon {
                argv.push("--icon".to_owned());
                argv.push(icon.to_owned());
            }
            continue;
        }
        let mut expanded = String::new();
        let mut chars = arg.chars();
        while let Some(ch) = chars.next() {
            if ch != '%' {
                expanded.push(ch);
                continue;
            }
            match chars.next() {
                Some('%') => expanded.push('%'),
                Some('c') => expanded.push_str(name),
                Some('k') => expanded.push_str(&path.display().to_string()),
                Some('f' | 'u' | 'd' | 'n' | 'v' | 'm') => {}
                Some(code) => return Err(format!("unknown field code %{code}")),
                None => return Err("trailing % in Exec".to_owned()),
            }
        }
        argv.push(expanded);
    }
    if argv.is_empty() {
        return Err("empty argv".to_owned());
    }
    Ok(argv)
}

#[cfg(test)]
mod tests {
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
    fn resolve_file_treats_empty_icon_and_path_as_absent() {
        let (_dir, path) = desktop(
            "[Desktop Entry]\nType=Application\nName=Pomodoro\nIcon=\nPath=\nExec=glimpse-applet %i\nImplements=me.aresa.Glimpse.Applet1;\n",
        );
        let entry = DesktopCatalog.resolve_file(&path).expect("entry");
        assert_eq!(entry.icon, None);
        assert_eq!(entry.cwd, None);
        assert_eq!(entry.argv.len(), 1);
    }
}
