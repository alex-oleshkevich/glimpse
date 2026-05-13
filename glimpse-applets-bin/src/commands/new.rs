use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;
use std::fs;
use std::path::{Path, PathBuf};

use crate::project::Language;

#[derive(clap::ValueEnum, Clone, Copy, Default, Debug)]
pub enum AppletType {
    #[default]
    Exec,
    Command,
}

impl AppletType {
    fn as_str(self) -> &'static str {
        match self {
            AppletType::Exec => "exec",
            AppletType::Command => "command",
        }
    }
}

#[derive(ClapArgs)]
pub struct Args {
    /// Project name. Becomes the directory name; must be a valid identifier
    /// for the target language's package system.
    name: String,
    /// Language to scaffold. Defaults to rust.
    #[arg(long, value_enum, default_value_t = Language::Rust)]
    lang: Language,
    /// Applet type: exec (runs a long-lived process) or command (runs commands on interaction).
    #[arg(long, value_enum, default_value_t = AppletType::Exec)]
    r#type: AppletType,
    /// Parent directory the project is created inside. Defaults to the
    /// current working directory.
    #[arg(short, long, value_name = "DIR")]
    dir: Option<PathBuf>,
    /// Allow scaffolding into an existing non-empty directory.
    #[arg(long)]
    force: bool,
}

pub fn run(args: Args) -> Result<()> {
    validate_name(&args.name, args.lang, args.r#type)?;

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
        format!(
            "failed to create project directory {}",
            project_dir.display()
        )
    })?;

    if matches!(args.r#type, AppletType::Exec) {
        let files = templates_for(args.lang);
        for (rel, content) in files {
            let body = render(content, &args.name);
            let dst = project_dir.join(rel);
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&dst, body).with_context(|| format!("write {}", dst.display()))?;
        }
    }

    let toml_path = project_dir.join("applet.toml");
    let applet_toml = render(applet_toml_template(args.r#type), &args.name);
    fs::write(&toml_path, applet_toml).with_context(|| format!("write {}", toml_path.display()))?;

    print_next_steps(args.lang, args.r#type, &project_dir, &args.name);
    Ok(())
}

fn validate_name(name: &str, lang: Language, applet_type: AppletType) -> Result<()> {
    if name.is_empty() {
        bail!("project name must not be empty");
    }
    if name.starts_with('-') || name.starts_with('.') {
        bail!("project name must not start with '-' or '.'");
    }
    let ok = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !ok {
        bail!("project name {name:?} contains characters other than [A-Za-z0-9_-]");
    }
    if matches!(applet_type, AppletType::Exec) && lang == Language::Python && name.contains('-') {
        bail!(
            "python project name {name:?} contains a hyphen. use underscores so the import name matches."
        );
    }
    Ok(())
}

fn applet_toml_template(applet_type: AppletType) -> &'static str {
    match applet_type {
        AppletType::Exec => include_str!("../../templates/exec/applet.toml"),
        AppletType::Command => include_str!("../../templates/command/applet.toml"),
    }
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
            (
                "Cargo.toml",
                include_str!("../../templates/rust/Cargo.toml"),
            ),
            (
                "src/main.rs",
                include_str!("../../templates/rust/src/main.rs"),
            ),
            (
                ".gitignore",
                include_str!("../../templates/rust/.gitignore"),
            ),
        ],
        Language::Python => &[
            (
                "pyproject.toml",
                include_str!("../../templates/python/pyproject.toml"),
            ),
            ("main.py", include_str!("../../templates/python/main.py")),
            (
                ".gitignore",
                include_str!("../../templates/python/.gitignore"),
            ),
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

fn print_next_steps(lang: Language, applet_type: AppletType, project_dir: &Path, name: &str) {
    let absolute = project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf());

    println!(
        "Created {} ({} applet)",
        project_dir.display(),
        applet_type.as_str()
    );
    println!();
    println!("Edit applet.toml to configure the applet.");
    println!();
    println!("Next steps:");
    println!("  cd {}", project_dir.display());
    match applet_type {
        AppletType::Command => {
            println!();
            println!("Fill in icon and click commands in applet.toml, then link it:");
            println!("  glimpse-applet link");
        }
        AppletType::Exec => {
            match lang {
                Language::Rust => {
                    println!("  cargo build --release");
                    println!();
                    println!("Then set the command in applet.toml:");
                    println!(
                        "  command = [\"{}/target/release/{name}\"]",
                        absolute.display()
                    );
                }
                Language::Python => {
                    println!("  uv sync         # or: pip install -e .");
                    println!("  uv run main.py  # smoke test");
                    println!();
                    println!("Then set the command in applet.toml:");
                    println!(
                        "  command = [\"uv\", \"run\", \"--directory\", \"{}\", \"python\", \"main.py\"]",
                        absolute.display()
                    );
                }
                Language::Typescript => {
                    println!("  npm install");
                    println!("  npm run build");
                    println!();
                    println!("Then set the command in applet.toml:");
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
                    println!("Then set the command in applet.toml:");
                    println!("  command = [\"{}/{name}\"]", absolute.display());
                }
            }
            println!();
            println!("Link applet.toml into the Glimpse applets directory:");
            println!("  glimpse-applet link");
            println!("or run the dev server for live-reload:");
            println!("  glimpse-applet dev {}", absolute.display());
        }
    }
}
