use std::path::{Component, Path, PathBuf};

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
        entry_of(&info, id, None)
    }
}

impl DesktopCatalog {
    pub fn resolve_file(&self, path: &Path) -> Result<Entry, String> {
        let subject = path.display().to_string();
        if let Some(info) = DesktopAppInfo::from_filename(path) {
            return entry_of(&info, &subject, None);
        }
        if !path.exists() {
            return Err(format!("{subject}: no desktop entry"));
        }
        let info = relative_exec_info(path).ok_or_else(|| {
            format!("{subject}: cannot be loaded (not Type=Application, or its TryExec/Exec program is not on PATH)")
        })?;
        entry_of(&info, &subject, Some(path.to_path_buf()))
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
            let resolved = entry_of(&info, &id, None);
            rows.push((id, resolved));
        }
        rows.sort_by(|left, right| left.0.cmp(&right.0));
        rows
    }
}

fn join_relative(base: &Path, relative: &Path) -> PathBuf {
    base.join(
        relative
            .components()
            .filter(|component| !matches!(component, Component::CurDir))
            .collect::<PathBuf>(),
    )
}

fn relative_exec_info(path: &Path) -> Option<DesktopAppInfo> {
    let keyfile = gio::glib::KeyFile::new();
    keyfile
        .load_from_file(path, gio::glib::KeyFileFlags::NONE)
        .ok()?;
    let exec = keyfile.string("Desktop Entry", "Exec").ok()?;
    let mut argv: Vec<String> = gio::glib::shell_parse_argv(&exec)
        .ok()?
        .iter()
        .map(|arg| arg.to_str().map(str::to_owned))
        .collect::<Option<_>>()?;
    let first = PathBuf::from(argv.first()?);
    if first.is_absolute() || !first.to_string_lossy().contains('/') {
        return None;
    }
    let base = keyfile
        .string("Desktop Entry", "Path")
        .ok()
        .filter(|cwd| !cwd.is_empty())
        .map(PathBuf::from)
        .or_else(|| path.canonicalize().ok()?.parent().map(Path::to_path_buf))?;
    argv[0] = join_relative(&base, &first).display().to_string();
    let quoted: Vec<String> = argv
        .iter()
        .map(|arg| gio::glib::shell_quote(arg).to_str().map(str::to_owned))
        .collect::<Option<_>>()?;
    keyfile.set_string("Desktop Entry", "Exec", &quoted.join(" "));
    DesktopAppInfo::from_keyfile(&keyfile)
}

fn entry_of(
    info: &DesktopAppInfo,
    subject: &str,
    path_override: Option<PathBuf>,
) -> Result<Entry, String> {
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
    let path = match path_override {
        Some(path) => path,
        None => info
            .filename()
            .ok_or_else(|| format!("{subject}: no filename"))?,
    };
    let cwd = info
        .string("Path")
        .filter(|cwd| !cwd.is_empty())
        .map(|cwd| PathBuf::from(cwd.as_str()))
        .or_else(|| path.canonicalize().ok()?.parent().map(Path::to_path_buf));
    let mut argv = expand(&exec, &name, icon.as_deref(), &path)
        .map_err(|error| format!("{subject}: {error}"))?;
    if let Some(first) = argv.first_mut() {
        let program = Path::new(first.as_str());
        if !program.is_absolute()
            && first.contains('/')
            && let Some(cwd) = &cwd
        {
            *first = join_relative(cwd, program).display().to_string();
        }
    }
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
