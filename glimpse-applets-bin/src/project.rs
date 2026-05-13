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

    pub fn name(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::Typescript => "typescript",
            Self::Go => "go",
        }
    }

    /// Detect a project's language by walking up from `dir` looking for a
    /// language-defining manifest. Errors if zero or multiple are found in
    /// the same directory; the caller can disambiguate with `--lang`.
    pub fn detect(dir: &Path) -> Result<(Self, PathBuf)> {
        let mut found: Vec<(Self, PathBuf)> = Vec::new();
        for lang in [Self::Rust, Self::Python, Self::Typescript, Self::Go] {
            let path = dir.join(lang.manifest());
            if path.is_file() {
                found.push((lang, path));
            }
        }
        match found.len() {
            0 => Err(anyhow!(
                "no language manifest found in {}. expected one of: Cargo.toml, pyproject.toml, package.json, go.mod",
                dir.display()
            )),
            1 => Ok(found.remove(0)),
            _ => {
                let names: Vec<&str> = found.iter().map(|(l, _)| l.name()).collect();
                Err(anyhow!(
                    "multiple language manifests in {}: {}. pass --lang to choose.",
                    dir.display(),
                    names.join(", ")
                ))
            }
        }
    }
}
