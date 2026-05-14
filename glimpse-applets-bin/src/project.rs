//! Shared types for identifying applet projects.

use anyhow::{Result, anyhow};
use clap::ValueEnum;
use std::path::{Path, PathBuf};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Language {
    #[default]
    Rust,
    Python,
    Typescript,
    Go,
}

impl Language {
    pub fn manifest(self) -> &'static str {
        match self {
            Self::Rust => "Cargo.toml",
            Self::Python => "pyproject.toml",
            Self::Typescript => "package.json",
            Self::Go => "go.mod",
        }
    }

    pub fn entrypoint(self) -> &'static str {
        match self {
            Self::Rust => "src/main.rs",
            Self::Python => "main.py",
            Self::Typescript => "src/main.ts",
            Self::Go => "main.go",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::Typescript => "typescript",
            Self::Go => "go",
        }
    }

    /// Detect a project's language from `dir`. First tries manifest files
    /// (Cargo.toml, pyproject.toml, package.json, go.mod); if none are found,
    /// falls back to entry-point filenames (main.py, main.go, etc.). Pass
    /// `--lang` to the caller to override when detection is ambiguous.
    pub fn detect(dir: &Path) -> Result<(Self, PathBuf)> {
        let all = [Self::Rust, Self::Python, Self::Typescript, Self::Go];

        let mut by_manifest: Vec<(Self, PathBuf)> = all
            .iter()
            .filter_map(|&lang| {
                let p = dir.join(lang.manifest());
                p.is_file().then_some((lang, p))
            })
            .collect();

        if by_manifest.len() == 1 {
            return Ok(by_manifest.remove(0));
        }
        if by_manifest.len() > 1 {
            let names: Vec<&str> = by_manifest.iter().map(|(l, _)| l.name()).collect();
            return Err(anyhow!(
                "multiple language manifests in {}: {}. pass --lang to choose.",
                dir.display(),
                names.join(", ")
            ));
        }

        // No manifest — try entry-point files.
        let mut by_entrypoint: Vec<(Self, PathBuf)> = all
            .iter()
            .filter_map(|&lang| {
                let p = dir.join(lang.entrypoint());
                p.is_file().then_some((lang, p))
            })
            .collect();

        match by_entrypoint.len() {
            0 => Err(anyhow!(
                "no language manifest found in {}. expected one of: Cargo.toml, pyproject.toml, package.json, go.mod",
                dir.display()
            )),
            1 => Ok(by_entrypoint.remove(0)),
            _ => {
                let names: Vec<&str> = by_entrypoint.iter().map(|(l, _)| l.name()).collect();
                Err(anyhow!(
                    "multiple entry points in {}: {}. pass --lang to choose.",
                    dir.display(),
                    names.join(", ")
                ))
            }
        }
    }
}
