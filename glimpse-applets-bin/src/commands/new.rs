use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;
use std::fs;
use std::path::{Path, PathBuf};

use crate::project::Language;

#[derive(ClapArgs)]
pub struct Args {
    /// Language to scaffold (rust, python, typescript, or go).
    #[arg(value_enum)]
    lang: Language,
    /// Project name. Becomes the directory name; must be a valid identifier
    /// for the target language's package system.
    name: String,
    /// Parent directory the project is created inside. Defaults to the
    /// current working directory.
    #[arg(short, long, value_name = "DIR")]
    dir: Option<PathBuf>,
    /// Allow scaffolding into an existing non-empty directory.
    #[arg(long)]
    force: bool,
}

pub fn run(args: Args) -> Result<()> {
    validate_name(&args.name, args.lang)?;

    let parent = args.dir.unwrap_or_else(|| PathBuf::from("."));
    let project_dir = parent.join(&args.name);

    if project_dir.exists() && !args.force {
        if project_dir.is_dir() && fs::read_dir(&project_dir)?.next().is_none() {
            // empty dir — ok
        } else {
            bail!(
                "{} already exists. pass --force to scaffold into it anyway.",
                project_dir.display()
            );
        }
    }

    fs::create_dir_all(&project_dir).with_context(|| {
        format!("failed to create project directory {}", project_dir.display())
    })?;

    let files = templates_for(args.lang);
    for (rel, content) in files {
        let body = render(content, &args.name);
        let dst = project_dir.join(rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dst, body).with_context(|| format!("write {}", dst.display()))?;
    }

    print_next_steps(args.lang, &project_dir, &args.name);
    Ok(())
}

fn validate_name(name: &str, lang: Language) -> Result<()> {
    if name.is_empty() {
        bail!("project name must not be empty");
    }
    if name.starts_with('-') || name.starts_with('.') {
        bail!("project name must not start with '-' or '.'");
    }
    let ok = name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !ok {
        bail!("project name {name:?} contains characters other than [A-Za-z0-9_-]");
    }
    if matches!(lang, Language::Rust | Language::Python) && name.contains('-') && lang == Language::Python {
        // python module names can't contain hyphens
        bail!(
            "python project name {name:?} contains a hyphen. use underscores so the import name matches."
        );
    }
    Ok(())
}

fn render(template: &str, name: &str) -> String {
    let py_module = name.replace('-', "_");
    template
        .replace("__NAME__", name)
        .replace("__NAME_PY__", &py_module)
}

fn templates_for(lang: Language) -> &'static [(&'static str, &'static str)] {
    match lang {
        Language::Rust => &[
            ("Cargo.toml", include_str!("../../templates/rust/Cargo.toml")),
            ("src/main.rs", include_str!("../../templates/rust/src/main.rs")),
            (".gitignore", include_str!("../../templates/rust/.gitignore")),
        ],
        Language::Python => &[
            (
                "pyproject.toml",
                include_str!("../../templates/python/pyproject.toml"),
            ),
            ("main.py", include_str!("../../templates/python/main.py")),
            (".gitignore", include_str!("../../templates/python/.gitignore")),
        ],
        Language::Typescript => &[
            (
                "package.json",
                include_str!("../../templates/typescript/package.json"),
            ),
            (
                "tsconfig.json",
                include_str!("../../templates/typescript/tsconfig.json"),
            ),
            (
                "src/main.ts",
                include_str!("../../templates/typescript/src/main.ts"),
            ),
            (
                ".gitignore",
                include_str!("../../templates/typescript/.gitignore"),
            ),
        ],
        Language::Go => &[
            ("main.go", include_str!("../../templates/go/main.go")),
            (".gitignore", include_str!("../../templates/go/.gitignore")),
        ],
    }
}

fn print_next_steps(lang: Language, project_dir: &Path, name: &str) {
    let absolute = project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf());

    println!("Created {} ({} applet)", project_dir.display(), lang.name());
    println!();
    println!("Next steps:");
    println!("  cd {}", project_dir.display());
    match lang {
        Language::Rust => {
            println!("  cargo build --release");
            println!();
            println!("Binary will be at: target/release/{name}");
            println!();
            println!("Add to ~/.config/glimpse/config.toml:");
            println!("  [applets.{name}]");
            println!("  extends = \"exec\"");
            println!(
                "  command = [\"{}/target/release/{name}\"]",
                absolute.display()
            );
        }
        Language::Python => {
            println!("  uv sync         # or: pip install -e .");
            println!("  uv run main.py  # smoke test");
            println!();
            println!("Add to ~/.config/glimpse/config.toml:");
            println!("  [applets.{name}]");
            println!("  extends = \"exec\"");
            println!(
                "  command = [\"uv\", \"run\", \"--directory\", \"{}\", \"python\", \"main.py\"]",
                absolute.display()
            );
        }
        Language::Typescript => {
            println!("  npm install");
            println!("  npm run build");
            println!();
            println!("Add to ~/.config/glimpse/config.toml:");
            println!("  [applets.{name}]");
            println!("  extends = \"exec\"");
            println!(
                "  command = [\"node\", \"{}/dist/main.js\"]",
                absolute.display()
            );
        }
        Language::Go => {
            println!("  go mod init example.com/{name}");
            println!("  go get github.com/alex-oleshkevich/glimpse/sdk/sdk-go");
            println!("  go build -o {name}");
            println!();
            println!("Add to ~/.config/glimpse/config.toml:");
            println!("  [applets.{name}]");
            println!("  extends = \"exec\"");
            println!(
                "  command = [\"{}/{name}\"]",
                absolute.display()
            );
        }
    }
    println!();
    println!("Or for live-reload during development:");
    println!("  command = [\"glimpse-applet\", \"dev\", \"{}\"]", absolute.display());
}

