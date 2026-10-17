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
const EXCLUDED_DIRS: [&str; 5] = [".git", "target", "node_modules", "__pycache__", ".venv"];

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

enum Language {
    Rs { bin: String },
    Go { bin: String },
    Ts,
    Source,
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
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
            if EXCLUDED_DIRS
                .iter()
                .any(|name| entry.file_name() == std::ffi::OsStr::new(name))
            {
                continue;
            }
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

fn run_build(argv: &[&str], dir: &Path) -> Result<()> {
    let (program, args) = argv.split_first().context("empty build command")?;
    let status = Command::new(program)
        .args(args)
        .current_dir(dir)
        .status()
        .with_context(|| format!("cannot run build command: {}", argv.join(" ")))?;
    ensure!(status.success(), "build failed: {}", argv.join(" "));
    Ok(())
}

fn rewrite_exec(desktop_path: &Path, exec: &str) -> Result<()> {
    let desktop = glib::KeyFile::new();
    desktop.load_from_file(desktop_path, glib::KeyFileFlags::KEEP_TRANSLATIONS)?;
    desktop.set_string(GROUP, "Exec", exec);
    fs::write(desktop_path, desktop.to_data())?;
    Ok(())
}

fn cargo_build(dir: &Path) -> Result<(String, PathBuf)> {
    run_build(&["cargo", "build", "--release"], dir)?;

    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(dir)
        .output()
        .context("cannot run cargo metadata")?;
    ensure!(output.status.success(), "cargo metadata failed");

    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let manifest = dir.join("Cargo.toml").canonicalize()?;
    let package = parsed["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|package| {
            package["manifest_path"]
                .as_str()
                .is_some_and(|path| Path::new(path) == manifest.as_path())
        })
        .context("cargo metadata does not describe this directory")?;

    let bin = package["targets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|target| {
            target["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind.as_str() == Some("bin")))
        })
        .and_then(|target| target["name"].as_str())
        .context("cargo package builds no binary")?
        .to_owned();

    let target_dir = PathBuf::from(
        parsed["target_directory"]
            .as_str()
            .context("cargo metadata names no target directory")?,
    );
    let built = target_dir.join("release").join(&bin);
    ensure!(
        built.is_file(),
        "cargo produced no binary at {}",
        built.display()
    );
    Ok((bin, built))
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

    #[cfg(test)]
    fn with_dirs(applets_dir: PathBuf, applications_dir: PathBuf) -> Self {
        Self {
            applets_dir,
            applications_dir,
        }
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
        ensure!(
            !dir.join(METADATA_FILE).exists(),
            "{METADATA_FILE} is generated at package time; remove it from the applet directory"
        );
        let (id, entry) = super::run::source(&dir)?;
        settings(&dir, &id)?;

        let mut built: Option<PathBuf> = None;
        let language = if dir.join("Cargo.toml").is_file() {
            let (bin, artifact) = cargo_build(&dir)?;
            built = Some(artifact);
            Language::Rs { bin }
        } else if dir.join("go.mod").is_file() {
            Language::Go {
                bin: super::applets_author::slug(&id),
            }
        } else if dir.join("package.json").is_file() {
            let parsed: serde_json::Value =
                serde_json::from_slice(&fs::read(dir.join("package.json"))?)?;
            if parsed["scripts"]["build"].is_string() {
                run_build(&["node", "--run", "build"], &dir)?;
            }
            Language::Ts
        } else {
            Language::Source
        };

        let mut sources = Vec::new();
        collect(&dir, &dir, &mut sources)?;
        for relative in sources {
            if relative == Path::new("devconfig.toml")
                || relative
                    .to_str()
                    .is_some_and(|name| name.ends_with(".glimpse.zip"))
            {
                continue;
            }
            let target = staging.join(&relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(dir.join(&relative), &target)?;
        }

        let desktop = staging.join(entry.path.file_name().context("desktop file name")?);
        match &language {
            Language::Rs { bin } => {
                let artifact = built.context("cargo build produced no binary")?;
                fs::copy(&artifact, staging.join(bin))?;
                fs::set_permissions(staging.join(bin), fs::Permissions::from_mode(0o755))?;
                rewrite_exec(&desktop, &format!("./{bin}"))?;
            }
            Language::Go { bin } => {
                let target = staging.join(bin);
                let target_text = target.to_str().context("build path is not UTF-8")?;
                run_build(&["go", "build", "-o", target_text, "."], &dir)?;
                rewrite_exec(&desktop, &format!("./{bin}"))?;
            }
            Language::Ts => {
                if staging.join("build/applet.mjs").is_file() {
                    rewrite_exec(&desktop, "node build/applet.mjs")?;
                }
            }
            Language::Source => {}
        }

        let mut staged = Vec::new();
        collect(staging, staging, &mut staged)?;

        let mut files = Vec::new();
        let mut total: u64 = 0;
        for relative in staged {
            let bytes = fs::read(staging.join(&relative))?;
            total += bytes.len() as u64;
            ensure!(
                total <= MAX_PACKAGE_BYTES,
                "package exceeds {MAX_PACKAGE_BYTES} bytes"
            );
            files.push(FileEntry {
                path: relative
                    .to_str()
                    .context("applet file name is not UTF-8")?
                    .to_owned(),
                sha256: sha256_hex(&bytes),
            });
        }

        let metadata = Metadata { id, files };
        fs::write(staging.join(METADATA_FILE), toml::to_string(&metadata)?)?;
        Ok(metadata)
    }

    pub fn package(&self, dir: &Path, out: Option<&Path>) -> Result<(Metadata, PathBuf)> {
        let staging = tempfile::tempdir()?;
        let metadata = self.stage(dir, staging.path())?;
        let out = match out {
            Some(out) => out.to_path_buf(),
            None => std::env::current_dir()?.join(format!("{}.{PACKAGE_EXTENSION}", metadata.id)),
        };
        let mut names: Vec<String> = metadata
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect();
        names.push(METADATA_FILE.to_owned());
        names.sort();

        let mut archive = zip::ZipWriter::new(File::create(&out)?);
        for name in names {
            let source = staging.path().join(&name);
            let mode = fs::metadata(&source)?.permissions().mode();
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(mode);
            archive.start_file(&name, options)?;
            archive.write_all(&fs::read(&source)?)?;
        }
        archive.finish()?;
        Ok((metadata, out))
    }

    fn unstage(&self, package: &Path, staging: &Path) -> Result<Metadata> {
        let mut archive = zip::ZipArchive::new(File::open(package)?)?;
        let mut text = String::new();
        archive
            .by_name(METADATA_FILE)
            .map_err(|_| Refused("package has no metadata.toml".to_owned()))?
            .read_to_string(&mut text)?;

        let metadata: Metadata = toml::from_str(&text)
            .map_err(|error| Refused(format!("invalid metadata.toml: {error}")))?;
        ensure!(
            is_desktop_id(&metadata.id),
            "invalid desktop-file id: {}",
            metadata.id
        );

        let mut present = BTreeSet::new();
        for index in 0..archive.len() {
            let entry = archive.by_index(index)?;
            ensure!(!entry.is_dir(), "package contains a directory entry");
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
        for file in &metadata.files {
            let relative = enclosed(&file.path).map_err(|_| {
                Refused(format!(
                    "metadata entry escapes the install directory: {}",
                    file.path
                ))
            })?;

            let mut entry = archive.by_name(&file.path)?;
            total += entry.size();
            ensure!(
                total <= MAX_PACKAGE_BYTES,
                "package exceeds {MAX_PACKAGE_BYTES} bytes"
            );

            let mode = entry.unix_mode().unwrap_or(0o644);
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            ensure!(
                sha256_hex(&bytes) == file.sha256,
                "{}: sha256 does not match metadata.toml",
                file.path
            );

            let target = staging.join(&relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&target, &bytes)?;
            fs::set_permissions(
                &target,
                fs::Permissions::from_mode(if mode & 0o111 != 0 { 0o755 } else { 0o644 }),
            )?;
        }
        fs::write(staging.join(METADATA_FILE), &text)?;
        Ok(metadata)
    }

    fn remove_listed(&self, dir: &Path, metadata: &Metadata) -> Result<Vec<PathBuf>> {
        for file in &metadata.files {
            let relative = enclosed(&file.path)?;
            match fs::remove_file(dir.join(&relative)) {
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

    fn install_staged(&self, staging: &Path, metadata: &Metadata) -> Result<Installed> {
        let target = self.applets_dir.join(&metadata.id);
        if target.exists() {
            let previous = self.metadata(&metadata.id).map_err(|_| {
                Refused(format!(
                    "{} exists and was not installed by glimpsectl",
                    target.display()
                ))
            })?;
            let leftovers = self.remove_listed(&target, &previous)?;
            ensure!(
                leftovers.is_empty(),
                "{} holds files the previous install does not own",
                target.display()
            );
        }

        let mut staged = Vec::new();
        collect(staging, staging, &mut staged)?;

        for relative in staged {
            let destination = target.join(&relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(staging.join(&relative), &destination)?;
        }

        let installed = Installed {
            id: metadata.id.clone(),
            dir: target.clone(),
        };

        if let Err(error) = self.install_desktop(metadata, &target) {
            let _ = self.remove_listed(&target, metadata);
            return Err(error);
        }
        Ok(installed)
    }

    fn install_desktop(&self, metadata: &Metadata, target: &Path) -> Result<()> {
        let name = format!("{}.desktop", metadata.id);
        let source = target.join(&name);
        ensure!(source.is_file(), "package has no {name}");

        let desktop = glib::KeyFile::new();
        desktop.load_from_file(&source, glib::KeyFileFlags::KEEP_TRANSLATIONS)?;

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
        fs::create_dir_all(&self.applications_dir)?;

        let destination = self.applications_dir.join(&name);
        if destination.exists() {
            marked(&destination)
                .map_err(|_| Refused(format!("refusing to overwrite {}", destination.display())))?;
        }

        fs::write(&destination, desktop.to_data())?;
        if let Err(reason) = DesktopCatalog.resolve_file(&destination) {
            let _ = fs::remove_file(&destination);
            anyhow::bail!("{reason}");
        }
        Ok(())
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
        let mut leftovers = Vec::new();
        let mut removed_files = false;

        if target.join(METADATA_FILE).is_file() {
            let metadata = self.metadata(id)?;
            leftovers = self.remove_listed(&target, &metadata)?;
            removed_files = true;
        }

        let desktop = self.applications_dir.join(format!("{id}.desktop"));
        let mut removed_desktop = false;
        if desktop.exists() || desktop.is_symlink() {
            let (_, opened) = marked(&desktop)?;
            let current = fs::symlink_metadata(&desktop)?;
            ensure!(
                current.is_file() && (opened.dev(), opened.ino()) == (current.dev(), current.ino()),
                "desktop entry changed during uninstall"
            );
            fs::remove_file(&desktop)?;
            removed_desktop = true;
        }

        ensure!(
            removed_files || removed_desktop,
            "{id} is not installed by glimpsectl"
        );
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
    eprintln!(
        "Add [applets.\"{0}\"] with applet = \"{0}\", then add \"{0}\" to a panel zone.",
        installed.id
    );
    Ok(())
}

pub fn applet_package(args: PackageArgs, json: bool) -> Result<()> {
    let manager = PackageManager::new()?;
    let (metadata, path) = manager.package(&args.dir, args.out.as_deref())?;
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
