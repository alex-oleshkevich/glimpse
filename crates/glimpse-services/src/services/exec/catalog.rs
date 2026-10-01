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
        .map(|cwd| PathBuf::from(cwd.as_str()))
        .or_else(|| path.canonicalize().ok()?.parent().map(Path::to_path_buf));
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
mod tests;
