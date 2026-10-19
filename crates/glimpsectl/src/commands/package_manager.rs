use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, ensure};
use glimpse_config::is_desktop_id;
use glimpse_services::DesktopCatalog;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::applet_tools::{GROUP, MARKER, marked, settings};
use crate::cli::PackageArgs;
use crate::errors::Refused;

const METADATA_FILE: &str = "metadata.toml";
const PACKAGE_EXTENSION: &str = "glimpse.zip";
const MAX_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_METADATA_BYTES: u64 = 1024 * 1024;
const MAX_ENTRIES: usize = 10_000;
const DIST_DIR: &str = "dist";

pub(super) fn has_justfile(dir: &Path) -> bool {
    ["justfile", "Justfile", ".justfile"]
        .iter()
        .any(|name| dir.join(name).is_file())
}

pub(super) fn just_build(dir: &Path) -> Command {
    let mut command = Command::new("just");
    command
        .arg("build")
        .current_dir(dir)
        .stdout(std::io::stderr());
    command
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileEntry {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub id: String,
    pub files: Vec<FileEntry>,
}

#[derive(Debug)]
pub struct Installed {
    pub id: String,
    pub dir: PathBuf,
}

#[derive(Debug)]
pub struct Removed {
    pub leftovers: Vec<PathBuf>,
}

pub struct PackageManager {
    applets_dir: PathBuf,
    applications_dir: PathBuf,
}

fn sha256_file(path: &Path) -> Result<(String, u64)> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut size: u64 = 0;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    Ok((format!("{:x}", hasher.finalize()), size))
}

fn enclosed(path: &str) -> Result<PathBuf> {
    let candidate = Path::new(path);
    ensure!(
        !path.is_empty()
            && candidate
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
        "package entry escapes the install directory: {path}"
    );
    Ok(candidate.to_path_buf())
}

fn collect(dir: &Path, base: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<std::io::Result<_>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type()?;
        ensure!(
            !file_type.is_symlink(),
            "applet directory contains a symlink: {}",
            path.display()
        );
        if file_type.is_dir() {
            collect(&path, base, files)?;
        } else if file_type.is_file() {
            files.push(path.strip_prefix(base)?.to_path_buf());
        } else {
            anyhow::bail!(
                "applet directory contains a non-regular file: {}",
                path.display()
            );
        }
    }
    Ok(())
}

fn copy_listed(from: &Path, to: &Path, files: &[PathBuf]) -> Result<()> {
    for relative in files {
        let destination = to.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(from.join(relative), &destination)?;
    }
    Ok(())
}

fn find_desktop(dir: &Path) -> Result<(String, PathBuf)> {
    let mut desktops = fs::read_dir(dir)?
        .filter_map(|item| item.ok().map(|item| item.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "desktop"));
    let path = desktops
        .next()
        .context("no .desktop file in applet directory")?;
    ensure!(
        desktops.next().is_none(),
        "multiple .desktop files in applet directory"
    );
    let id = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .context("invalid .desktop filename")?
        .to_owned();
    ensure!(is_desktop_id(&id), "invalid desktop-file id: {id}");
    Ok((id, path))
}

fn check_exec(dist: &Path, desktop_path: &Path) -> Result<()> {
    let desktop = glib::KeyFile::new();
    desktop.load_from_file(desktop_path, glib::KeyFileFlags::NONE)?;
    ensure!(
        desktop
            .string_list(GROUP, "Implements")
            .is_ok_and(|items| items
                .iter()
                .any(|item| item.as_str() == "me.aresa.Glimpse.Applet1")),
        "desktop entry does not implement me.aresa.Glimpse.Applet1"
    );
    let exec = desktop
        .string(GROUP, "Exec")
        .map_err(|_| Refused("desktop entry has no Exec".to_owned()))?;
    let argv = glib::shell_parse_argv(&exec)?;
    let first = argv
        .first()
        .and_then(|arg| arg.to_str())
        .unwrap_or_default();
    if let Some(rest) = first.strip_prefix("./") {
        ensure!(
            dist.join(rest).is_file(),
            "Exec names ./{rest}, which the build did not produce in {DIST_DIR}/"
        );
    }
    Ok(())
}

impl PackageManager {
    pub fn new() -> Result<Self> {
        let applets_dir =
            glimpse_config::applets_dir().context("no data directory for installed applets")?;
        Ok(Self {
            applets_dir,
            applications_dir: glib::user_data_dir().join("applications"),
        })
    }

    pub fn metadata(&self, id: &str) -> Result<Metadata> {
        ensure!(is_desktop_id(id), "invalid desktop-file id: {id}");
        let path = self.applets_dir.join(id).join(METADATA_FILE);
        let text = fs::read_to_string(&path)
            .with_context(|| format!("{id} is not installed by glimpsectl"))?;
        let metadata: Metadata = toml::from_str(&text)
            .with_context(|| format!("installed metadata for {id} is invalid"))?;
        ensure!(
            metadata.id == id,
            "installed metadata names {}",
            metadata.id
        );
        Ok(metadata)
    }

    fn stage(&self, dir: &Path, staging: &Path) -> Result<Metadata> {
        let dir = dir.canonicalize()?;
        let (id, _) = find_desktop(&dir)?;
        settings(&dir, &id)?;
        ensure!(
            has_justfile(&dir),
            "packaging runs `just build`; add a justfile whose build recipe assembles production files into {DIST_DIR}/"
        );
        let dist = dir.join(DIST_DIR);
        if dist.exists() {
            ensure!(
                !fs::symlink_metadata(&dist)?.file_type().is_symlink(),
                "{DIST_DIR}/ is a symlink"
            );
            fs::remove_dir_all(&dist)?;
        }
        eprintln!("build: just build (runs the applet's build recipe unconfined)");
        let status = just_build(&dir)
            .status()
            .context("cannot run `just build`")?;
        ensure!(status.success(), "`just build` failed");
        ensure!(
            dist.is_dir(),
            "`just build` produced no {DIST_DIR}/ directory"
        );
        let (dist_id, dist_desktop) = find_desktop(&dist)?;
        ensure!(dist_id == id, "{DIST_DIR}/ holds {dist_id}, expected {id}");
        check_exec(&dist, &dist_desktop)?;

        let mut produced = Vec::new();
        collect(&dist, &dist, &mut produced)?;
        ensure!(
            !produced.contains(&PathBuf::from(METADATA_FILE)),
            "{METADATA_FILE} is generated at package time; remove it from {DIST_DIR}/"
        );
        copy_listed(&dist, staging, &produced)?;

        let mut files = Vec::new();
        let mut total: u64 = 0;
        for relative in produced {
            let (sha256, size) = sha256_file(&staging.join(&relative))?;
            total += size;
            ensure!(
                total <= MAX_PACKAGE_BYTES,
                "package exceeds {MAX_PACKAGE_BYTES} bytes"
            );
            files.push(FileEntry {
                path: relative
                    .to_str()
                    .context("applet file name is not UTF-8")?
                    .to_owned(),
                sha256,
            });
        }
        let metadata = Metadata { id, files };
        fs::write(staging.join(METADATA_FILE), toml::to_string(&metadata)?)?;
        Ok(metadata)
    }

    pub fn package(&self, dir: &Path, out: Option<&Path>) -> Result<(Metadata, PathBuf)> {
        let source = dir.canonicalize()?;
        let staging = tempfile::tempdir()?;
        let metadata = self.stage(dir, staging.path())?;
        let file_name = format!("{}.{PACKAGE_EXTENSION}", metadata.id);
        let mut out = match out {
            Some(out) => std::env::current_dir()?.join(out),
            None => std::env::current_dir()?,
        };
        if out.is_dir() {
            out.push(file_name);
        }
        ensure!(
            !out.starts_with(&source)
                || out
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().ends_with(".glimpse.zip")),
            "package output inside the source directory: {}",
            out.display()
        );
        let mut names: Vec<String> = metadata
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect();
        names.push(METADATA_FILE.to_owned());
        names.sort();
        let temp = tempfile::NamedTempFile::new_in(
            out.parent()
                .context("package output has no parent directory")?,
        )?;
        let mut archive = zip::ZipWriter::new(temp);
        for name in names {
            let source_file = staging.path().join(&name);
            let mode = fs::metadata(&source_file)?.permissions().mode();
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(mode);
            archive.start_file(&name, options)?;
            std::io::copy(&mut File::open(&source_file)?, &mut archive)?;
        }
        let temp = archive.finish()?;
        temp.as_file()
            .set_permissions(fs::Permissions::from_mode(0o644))?;
        temp.persist(&out).map_err(|error| error.error)?;
        Ok((metadata, out))
    }

    fn unstage(&self, package: &Path, staging: &Path) -> Result<Metadata> {
        let mut archive = zip::ZipArchive::new(File::open(package)?)?;
        ensure!(
            archive.len() <= MAX_ENTRIES,
            "package holds more than {MAX_ENTRIES} entries"
        );
        let mut raw = Vec::new();
        archive
            .by_name(METADATA_FILE)
            .map_err(|_| Refused("package has no metadata.toml".to_owned()))?
            .take(MAX_METADATA_BYTES + 1)
            .read_to_end(&mut raw)?;
        ensure!(
            raw.len() as u64 <= MAX_METADATA_BYTES,
            "metadata.toml exceeds {MAX_METADATA_BYTES} bytes"
        );
        let text =
            String::from_utf8(raw).map_err(|_| Refused("metadata.toml is not UTF-8".to_owned()))?;
        let metadata: Metadata = toml::from_str(&text)
            .map_err(|error| Refused(format!("invalid metadata.toml: {error}")))?;
        ensure!(
            is_desktop_id(&metadata.id),
            "invalid desktop-file id: {}",
            metadata.id
        );

        let relatives: Vec<PathBuf> = metadata
            .files
            .iter()
            .map(|file| enclosed(&file.path))
            .collect::<Result<_>>()?;
        ensure!(
            relatives.iter().collect::<BTreeSet<_>>().len() == relatives.len(),
            "metadata.toml lists a file twice"
        );

        let mut present = BTreeSet::new();
        for index in 0..archive.len() {
            let entry = archive.by_index(index)?;
            ensure!(!entry.is_dir(), "package contains a directory entry");
            ensure!(!entry.is_symlink(), "package contains a symlink entry");
            ensure!(
                present.insert(entry.name()?.into_owned()),
                "package contains a duplicate entry"
            );
        }
        let expected: BTreeSet<String> = metadata
            .files
            .iter()
            .map(|file| file.path.clone())
            .chain([METADATA_FILE.to_owned()])
            .collect();
        ensure!(
            present == expected,
            "package contents do not match metadata.toml"
        );

        let mut total: u64 = 0;
        for (file, relative) in metadata.files.iter().zip(&relatives) {
            let mut entry = archive.by_name(&file.path)?;
            let mode = entry.unix_mode().unwrap_or(0o644);
            let target = staging.join(relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out_file = File::create(&target)?;
            let mut hasher = Sha256::new();
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let read = entry.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                total += read as u64;
                ensure!(
                    total <= MAX_PACKAGE_BYTES,
                    "package exceeds {MAX_PACKAGE_BYTES} bytes"
                );
                hasher.update(&buffer[..read]);
                out_file.write_all(&buffer[..read])?;
            }
            ensure!(
                format!("{:x}", hasher.finalize()) == file.sha256,
                "{}: sha256 does not match metadata.toml",
                file.path
            );
            fs::set_permissions(
                &target,
                fs::Permissions::from_mode(if mode & 0o111 != 0 { 0o755 } else { 0o644 }),
            )?;
        }
        fs::write(staging.join(METADATA_FILE), &text)?;
        Ok(metadata)
    }

    fn remove_listed(&self, dir: &Path, metadata: &Metadata) -> Result<Vec<PathBuf>> {
        let relatives: Vec<PathBuf> = metadata
            .files
            .iter()
            .map(|file| enclosed(&file.path))
            .collect::<Result<_>>()?;
        for relative in &relatives {
            match fs::remove_file(dir.join(relative)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            let mut parent = relative.parent();
            while let Some(current) = parent.filter(|path| !path.as_os_str().is_empty()) {
                let _ = fs::remove_dir(dir.join(current));
                parent = current.parent();
            }
        }
        let _ = fs::remove_file(dir.join(METADATA_FILE));
        match fs::remove_dir(dir) {
            Ok(()) => Ok(Vec::new()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(_) => {
                let mut leftovers = Vec::new();
                if dir.is_dir() {
                    collect(dir, dir, &mut leftovers)?;
                }
                Ok(leftovers.into_iter().map(|path| dir.join(path)).collect())
            }
        }
    }

    fn desktop_data(&self, source: &Path, target: &Path) -> Result<String> {
        let desktop = glib::KeyFile::new();
        desktop.load_from_file(source, glib::KeyFileFlags::KEEP_TRANSLATIONS)?;
        if let Ok(exec) = desktop.string(GROUP, "Exec")
            && let Ok(argv) = glib::shell_parse_argv(&exec)
        {
            let mut argv: Vec<String> = argv
                .iter()
                .map(|arg| arg.to_str().unwrap_or_default().to_owned())
                .collect();
            if let Some(first) = argv.first_mut()
                && let Some(rest) = first.strip_prefix("./")
            {
                *first = target.join(rest).display().to_string();
                let quoted: Vec<String> = argv
                    .iter()
                    .map(|arg| {
                        glib::shell_quote(arg)
                            .to_str()
                            .context("quoted Exec is not UTF-8")
                            .map(str::to_owned)
                    })
                    .collect::<Result<_>>()?;
                desktop.set_string(GROUP, "Exec", &quoted.join(" "));
            }
        }
        desktop.set_string(
            GROUP,
            "Path",
            target.to_str().context("applet path is not UTF-8")?,
        );
        desktop.set_string(GROUP, MARKER, "glimpsectl");
        Ok(desktop.to_data().to_string())
    }

    fn check_desktop_destination(&self, destination: &Path) -> Result<Option<Vec<u8>>> {
        match fs::symlink_metadata(destination) {
            Ok(found) => {
                ensure!(
                    !found.file_type().is_symlink(),
                    "refusing a symlink desktop entry: {}",
                    destination.display()
                );
                ensure!(found.is_file(), "desktop entry is not a regular file");
                marked(destination).map_err(|_| {
                    Refused(format!("refusing to overwrite {}", destination.display()))
                })?;
                Ok(Some(fs::read(destination)?))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn write_desktop(&self, destination: &Path, data: &[u8]) -> Result<()> {
        let mut temp = tempfile::NamedTempFile::new_in(&self.applications_dir)?;
        temp.write_all(data)?;
        temp.persist(destination).map_err(|error| error.error)?;
        Ok(())
    }

    fn install_staged(&self, staging: &Path, metadata: &Metadata) -> Result<Installed> {
        fs::create_dir_all(&self.applets_dir)?;
        fs::create_dir_all(&self.applications_dir)?;
        let target = self.applets_dir.join(&metadata.id);
        if let Ok(found) = fs::symlink_metadata(&target) {
            ensure!(
                !found.file_type().is_symlink(),
                "{} is a symlink",
                target.display()
            );
        }
        let name = format!("{}.desktop", metadata.id);
        let staged_desktop = staging.join(&name);
        ensure!(staged_desktop.is_file(), "package has no {name}");
        let data = self.desktop_data(&staged_desktop, &target)?;
        let destination = self.applications_dir.join(&name);
        let old_desktop = self.check_desktop_destination(&destination)?;

        let replaced = target.exists();
        if replaced {
            let previous = self.metadata(&metadata.id).map_err(|_| {
                Refused(format!(
                    "{} exists and was not installed by glimpsectl",
                    target.display()
                ))
            })?;
            let owned: BTreeSet<PathBuf> = previous
                .files
                .iter()
                .map(|file| enclosed(&file.path))
                .collect::<Result<_>>()?;
            let mut present = Vec::new();
            collect(&target, &target, &mut present)?;
            let unowned: Vec<String> = present
                .iter()
                .filter(|path| !owned.contains(*path) && *path != &PathBuf::from(METADATA_FILE))
                .map(|path| path.display().to_string())
                .collect();
            ensure!(
                unowned.is_empty(),
                "{} holds files the previous install does not own: {}",
                target.display(),
                unowned.join(", ")
            );
        }

        let pid = std::process::id();
        let fresh = self.applets_dir.join(format!(".{}.new-{pid}", metadata.id));
        let backup = self.applets_dir.join(format!(".{}.old-{pid}", metadata.id));
        let _ = fs::remove_dir_all(&fresh);
        let _ = fs::remove_dir_all(&backup);
        let mut staged = Vec::new();
        collect(staging, staging, &mut staged)?;
        if let Err(error) = copy_listed(staging, &fresh, &staged) {
            let _ = fs::remove_dir_all(&fresh);
            return Err(error);
        }

        if replaced {
            fs::rename(&target, &backup)?;
        }
        if let Err(error) = fs::rename(&fresh, &target) {
            let _ = fs::remove_dir_all(&fresh);
            if replaced {
                let _ = fs::rename(&backup, &target);
            }
            return Err(error.into());
        }
        let outcome = self
            .write_desktop(&destination, data.as_bytes())
            .and_then(|()| {
                DesktopCatalog
                    .resolve_file(&destination)
                    .map(|_| ())
                    .map_err(anyhow::Error::msg)
            });
        if let Err(error) = outcome {
            let _ = fs::remove_file(&destination);
            let _ = fs::remove_dir_all(&target);
            if replaced {
                let _ = fs::rename(&backup, &target);
            }
            if let Some(bytes) = &old_desktop {
                let _ = fs::write(&destination, bytes);
            }
            return Err(error);
        }
        if replaced {
            let _ = fs::remove_dir_all(&backup);
        }
        Ok(Installed {
            id: metadata.id.clone(),
            dir: target,
        })
    }

    pub fn install(&self, source: &Path) -> Result<Installed> {
        let staging = tempfile::tempdir()?;
        let metadata = if source.is_dir() {
            self.stage(source, staging.path())?
        } else {
            self.unstage(source, staging.path())?
        };
        self.install_staged(staging.path(), &metadata)
    }

    pub fn uninstall(&self, id: &str) -> Result<Removed> {
        ensure!(is_desktop_id(id), "invalid desktop-file id: {id}");
        let target = self.applets_dir.join(id);
        if let Ok(found) = fs::symlink_metadata(&target) {
            ensure!(
                !found.file_type().is_symlink(),
                "{} is a symlink",
                target.display()
            );
        }
        let metadata = if target.join(METADATA_FILE).is_file() {
            let metadata = self.metadata(id)?;
            for file in &metadata.files {
                enclosed(&file.path)?;
            }
            Some(metadata)
        } else {
            None
        };
        let desktop = self.applications_dir.join(format!("{id}.desktop"));
        let desktop_identity = match fs::symlink_metadata(&desktop) {
            Ok(_) => {
                let (_, opened) = marked(&desktop)?;
                Some((opened.dev(), opened.ino()))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        ensure!(
            metadata.is_some() || desktop_identity.is_some(),
            "{id} is not installed by glimpsectl"
        );

        let mut leftovers = Vec::new();
        if let Some(metadata) = &metadata {
            leftovers = self.remove_listed(&target, metadata)?;
        }
        if let Some(identity) = desktop_identity {
            let current = fs::symlink_metadata(&desktop)?;
            ensure!(
                current.is_file() && (current.dev(), current.ino()) == identity,
                "desktop entry changed during uninstall"
            );
            fs::remove_file(&desktop)?;
        }
        Ok(Removed { leftovers })
    }
}

fn print_installed(installed: &Installed, json: bool) -> Result<()> {
    if json {
        super::emit(&serde_json::json!({
            "id": installed.id,
            "dir": installed.dir,
        }))?;
    } else {
        crate::render::print(&installed.id)?;
    }
    eprintln!("Add \"{}\" to a panel zone.", installed.id);
    Ok(())
}

pub fn applet_package(args: PackageArgs, json: bool) -> Result<()> {
    let manager = PackageManager::new()?;
    let (metadata, path) = manager.package(&args.dir, args.out.as_deref())?;
    for file in &metadata.files {
        eprintln!("{}", file.path);
    }
    if json && !args.install {
        super::emit(&serde_json::json!({
            "id": metadata.id,
            "path": path,
        }))?;
    } else if !args.install {
        crate::render::print(&path.display().to_string())?;
    }
    if args.install {
        let installed = manager.install(&path)?;
        print_installed(&installed, json)?;
    }
    Ok(())
}

pub fn applet_install(target: PathBuf, json: bool) -> Result<()> {
    let installed = PackageManager::new()?.install(&target)?;
    print_installed(&installed, json)
}

pub fn applet_uninstall(id: String) -> Result<()> {
    let removed = PackageManager::new()?.uninstall(&id)?;
    if !removed.leftovers.is_empty() {
        let kept: Vec<String> = removed
            .leftovers
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        eprintln!("left in place: {}", kept.join(", "));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
