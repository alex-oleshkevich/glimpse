use std::ffi::OsString;
use std::path::{Path, PathBuf};

use clap::parser::ValueSource;
use clap::{Args, CommandFactory as _, FromArgMatches as _, Parser, Subcommand, ValueEnum};
use glimpse_config::Editor;
use glimpse_shot::doc::document::Tool;
use glimpse_shot::doc::geometry::parse_geometry;
use glimpse_shot::session::Target;
use glimpse_utils::{ConfigArg, LogArgs};
use shadow_rs::shadow;

#[path = "prime.rs"]
pub mod prime;

shadow!(build);

const AGENT_POINTER: &str = "Agents: run `glimpse-shot prime` for the full contract.";
const ANNOTATE_POINTER: &str = "JSON array of annotations in source buffer pixels; `glimpse-shot prime` prints the format and examples.";
const JSON_POINTER: &str = "See `glimpse-shot prime` for the result object and failure format.";

const EXAMPLES: &str = "Examples:
  glimpse-shot                                       open the screenshot app
  glimpse-shot --screen                              freeze the screen and mark it up
  glimpse-shot --window                              pick a window, then mark it up
  glimpse-shot --screen --json                       ask a human to mark something; read the marks
  glimpse-shot capture --copy --no-save              copy the focused output, no file, no UI
  glimpse-shot capture --window -o shot.png          the focused window, to a file
  glimpse-shot capture --region \"$(slurp)\" -o -      a selected rectangle, PNG on stdout
  glimpse-shot list --json                           outputs and windows with geometry";

#[derive(Debug, Parser)]
#[command(
    name = "glimpse-shot",
    about = "Take a screenshot, mark it up, save or copy it.",
    before_help = EXAMPLES,
    after_help = AGENT_POINTER,
    after_long_help = AGENT_POINTER,
    version = build::VERSION
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub gui: GuiArgs,

    #[command(flatten)]
    pub config: ConfigArg,

    #[command(flatten)]
    pub log: LogArgs,

    #[command(flatten)]
    pub color: colorchoice_clap::Color,
}

impl Cli {
    pub fn json(&self) -> bool {
        match &self.command {
            None => self.gui.json,
            Some(Command::Capture(args)) => args.json,
            Some(Command::Annotate(args)) => args.json,
            Some(Command::List { json }) => *json,
            Some(Command::Prime | Command::Portal | Command::Hold) => false,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Take a screenshot without any window or overlay")]
    Capture(CaptureArgs),
    #[command(about = "Annotate an existing image")]
    Annotate(AnnotateArgs),
    #[command(about = "Print the complete agent guide")]
    Prime,
    #[command(about = "List outputs and windows with their geometry")]
    List {
        #[arg(long, help = "Print one JSON object instead of a table.")]
        json: bool,
    },
    #[command(hide = true)]
    Portal,
    #[command(hide = true)]
    Hold,
}

#[derive(Debug, Clone, Default, Args)]
#[group(multiple = false)]
pub struct TargetArgs {
    #[arg(
        long,
        num_args = 0..=1,
        default_missing_value = "current",
        value_name = "NAME",
        help = "The current output, or the one named."
    )]
    pub screen: Option<String>,

    #[arg(
        long,
        num_args = 0..=1,
        value_name = "ID",
        help = "A window: picked by hand, the focused one, or the one with this id."
    )]
    pub window: Option<Option<String>>,

    #[arg(
        long,
        num_args = 0..=1,
        value_name = "GEOMETRY",
        help = "A rectangle as \"X,Y WxH\" in the compositor's logical layout, or dragged by hand."
    )]
    pub region: Option<Option<String>>,
}

impl TargetArgs {
    pub fn is_set(&self) -> bool {
        self.screen.is_some() || self.window.is_some() || self.region.is_some()
    }

    pub fn resolve(&self, headless: bool) -> Result<Target, String> {
        if let Some(screen) = &self.screen {
            return match screen.as_str() {
                "current" => Ok(Target::CurrentScreen),
                "all" => Err("--screen all is reserved and not accepted yet".to_owned()),
                "" => Err(
                    "--screen got an empty name; leave the value out for the current output"
                        .to_owned(),
                ),
                name => Ok(Target::Screen(name.to_owned())),
            };
        }
        if let Some(window) = &self.window {
            return match window.as_deref() {
                None if headless => Ok(Target::FocusedWindow),
                None => Ok(Target::PickWindow),
                Some("focused") => Ok(Target::FocusedWindow),
                Some(id) => id
                    .parse()
                    .map(Target::Window)
                    .map_err(|_| format!("--window takes a window id or \"focused\", got {id:?}")),
            };
        }
        if let Some(region) = &self.region {
            return match region.as_deref() {
                None if headless => Err("--region needs a geometry such as \"X,Y WxH\"".to_owned()),
                None => Ok(Target::PickRegion),
                Some("") => Err(
                    "--region got an empty geometry; leave the value out to drag one by hand"
                        .to_owned(),
                ),
                Some(geometry) => parse_geometry(geometry)
                    .map(Target::Region)
                    .map_err(|error| format!("{error}; expected \"X,Y WxH\" or \"X,Y,WxH\"")),
            };
        }
        Ok(if headless {
            Target::CurrentScreen
        } else {
            Target::App
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum ScaleMode {
    #[default]
    Buffer,
    Logical,
}

#[derive(Debug, Clone, Default, Args)]
pub struct GuiArgs {
    #[command(flatten)]
    pub target: TargetArgs,

    #[arg(long, value_parser = editor, value_name = "WHERE", help = "overlay or window [default: from [screenshot] editor]")]
    pub editor: Option<Editor>,

    #[arg(short, long, value_name = "FILE", help = "Where Ctrl+S saves.")]
    pub output: Option<PathBuf>,

    #[arg(
        long,
        value_enum,
        value_name = "TOOL",
        help = "Start with this tool instead of the arrow."
    )]
    pub tool: Option<Tool>,

    #[arg(
        short,
        long,
        value_name = "FILE",
        help = "Draw these annotations (JSON file or -); see --help for the format.",
        long_help = ANNOTATE_POINTER
    )]
    pub annotate: Option<PathBuf>,

    #[arg(
        short,
        long,
        value_name = "SECONDS",
        value_parser = clap::value_parser!(u32).range(0..=60),
        help = "Count down this long before freezing the screen, 0 to 60."
    )]
    pub delay: Option<u32>,

    #[arg(long, overrides_with = "no_cursor", help = "Include the pointer.")]
    pub cursor: bool,

    #[arg(long, overrides_with = "cursor", help = "Leave the pointer out.")]
    pub no_cursor: bool,

    #[arg(
        long,
        overrides_with = "no_flash",
        help = "Flash the screen on capture."
    )]
    pub flash: bool,

    #[arg(long, overrides_with = "flash", help = "Do not flash the screen.")]
    pub no_flash: bool,

    #[arg(
        long,
        overrides_with = "no_notify",
        help = "Post a notification when saved or copied."
    )]
    pub notify: bool,

    #[arg(long, overrides_with = "notify", help = "Post no notification.")]
    pub no_notify: bool,

    #[arg(
        long,
        value_enum,
        default_value_t = ScaleMode::Buffer,
        help = "Save at buffer size, or downscale to logical size."
    )]
    pub scale: ScaleMode,

    #[arg(long, help = "Print the result object when the human finishes.", long_help = JSON_POINTER)]
    pub json: bool,

    #[arg(
        long,
        value_name = "TEXT",
        help = "A banner saying what is being asked for, plain text."
    )]
    pub prompt: Option<String>,

    #[arg(
        long,
        value_name = "SECONDS",
        help = "Close the editor unanswered after this long, exit 7."
    )]
    pub timeout: Option<u32>,

    #[arg(long, hide = true, value_name = "APP")]
    pub portal: Option<String>,

    #[arg(long, hide = true)]
    pub gapplication_service: bool,

    #[arg(
        value_name = "FILE.png",
        conflicts_with_all = ["screen", "window", "region"],
        help = "Open this image on the Result page."
    )]
    pub file: Option<PathBuf>,

    #[arg(skip)]
    pub pick: bool,
}

#[derive(Debug, Clone, Args)]
#[command(after_help = AGENT_POINTER, after_long_help = AGENT_POINTER)]
pub struct AnnotateArgs {
    #[arg(value_name = "IMAGE", help = "Image to edit (PNG, JPEG, WebP, …).")]
    pub image: Option<PathBuf>,
    #[arg(
        long,
        conflicts_with = "headless",
        help = "Choose an image when IMAGE is omitted."
    )]
    pub pick: bool,
    #[arg(long, value_name = "MARKS", long_help = ANNOTATE_POINTER)]
    pub annotate: Option<PathBuf>,
    #[arg(long, requires = "annotate", conflicts_with_all = ["tool", "prompt", "timeout"], help = "Export without opening an editor; needs --annotate.")]
    pub headless: bool,
    #[arg(
        short = 'o',
        long,
        value_name = "FILE",
        help = "Where the PNG is saved; - is stdout with --headless."
    )]
    pub output: Option<PathBuf>,
    #[arg(long, requires = "headless", help = "Put the PNG on the clipboard.")]
    pub copy: bool,
    #[arg(long, requires_all = ["copy", "headless"], conflicts_with = "output", help = "Write no file; needs --copy.")]
    pub no_save: bool,
    #[arg(long, long_help = JSON_POINTER)]
    pub json: bool,
    #[arg(long, value_enum, value_name = "T")]
    pub tool: Option<Tool>,
    #[arg(
        long,
        value_name = "TEXT",
        help = "A banner saying what is being asked for, plain text."
    )]
    pub prompt: Option<String>,
    #[arg(
        long,
        value_name = "S",
        help = "Close the editor unanswered after this long, exit 7."
    )]
    pub timeout: Option<u32>,
}

impl AnnotateArgs {
    pub fn into_gui(self) -> GuiArgs {
        GuiArgs {
            file: self.image,
            pick: self.pick,
            annotate: self.annotate,
            output: self.output,
            json: self.json,
            tool: self.tool,
            prompt: self.prompt,
            timeout: self.timeout,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Default, Args)]
#[command(after_help = AGENT_POINTER, after_long_help = AGENT_POINTER)]
pub struct CaptureArgs {
    #[arg(skip)]
    pub image: Option<PathBuf>,
    #[command(flatten)]
    pub target: TargetArgs,

    #[arg(
        short,
        long,
        value_name = "FILE",
        help = "Where the PNG is written; - is stdout [default: [screenshot] directory and filename]."
    )]
    pub output: Option<PathBuf>,

    #[arg(long, help = "Put the PNG on the clipboard.")]
    pub copy: bool,

    #[arg(
        long,
        requires = "copy",
        conflicts_with = "output",
        help = "Write no file; needs --copy."
    )]
    pub no_save: bool,

    #[arg(
        short,
        long,
        value_name = "FILE",
        help = "Draw these annotations (JSON file or -); see --help for the format.",
        long_help = ANNOTATE_POINTER
    )]
    pub annotate: Option<PathBuf>,

    #[arg(
        short,
        long,
        value_name = "SECONDS",
        value_parser = clap::value_parser!(u32).range(0..=60),
        help = "Wait this long before capturing, 0 to 60; nothing is shown."
    )]
    pub delay: Option<u32>,

    #[arg(long, overrides_with = "no_cursor", help = "Include the pointer.")]
    pub cursor: bool,

    #[arg(long, overrides_with = "cursor", help = "Leave the pointer out.")]
    pub no_cursor: bool,

    #[arg(
        long,
        value_enum,
        default_value_t = ScaleMode::Buffer,
        help = "Save at buffer size, or downscale to logical size."
    )]
    pub scale: ScaleMode,

    #[arg(long, help = "Print the result object on stdout.", long_help = JSON_POINTER)]
    pub json: bool,
}

impl CaptureArgs {
    pub fn cursor(&self, configured: bool) -> bool {
        paired(self.cursor, self.no_cursor).unwrap_or(configured)
    }
}

impl GuiArgs {
    pub fn in_window(&self, configured: Editor) -> bool {
        self.portal.is_none()
            && self.prompt.is_none()
            && self.timeout.is_none()
            && self.output.as_deref() != Some(Path::new("-"))
            && self.editor.unwrap_or(configured) == Editor::Window
    }
}

pub(crate) fn paired(yes: bool, no: bool) -> Option<bool> {
    match (yes, no) {
        (true, _) => Some(true),
        (_, true) => Some(false),
        _ => None,
    }
}

fn editor(name: &str) -> Result<Editor, String> {
    match name {
        "overlay" => Ok(Editor::Overlay),
        "window" => Ok(Editor::Window),
        other => Err(format!("expected overlay or window, got {other:?}")),
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Terminal {
    pub stdin: bool,
    pub stdout: bool,
}

pub fn validate_gui(args: &GuiArgs, terminal: Terminal) -> Result<(), String> {
    let mut problems = Vec::new();
    let target = args.target.resolve(false);
    let imaged = args.file.is_some() || args.pick;
    let targeted = args.target.is_set() || imaged;
    if let Err(error) = &target {
        problems.push(error.clone());
    }
    if args.annotate.is_some() && !imaged && !concrete(target.as_ref().ok()) {
        problems.push("--annotate needs a screen, a window id or a region geometry: the marks need a known image".to_owned());
    }
    if !targeted && args.json {
        problems
            .push("--json needs a target: the app window has no single result to print".to_owned());
    }
    if !targeted && (args.prompt.is_some() || args.timeout.is_some()) {
        problems.push(
            "--prompt and --timeout need a target: the app window is not a question".to_owned(),
        );
    }
    if args.editor == Some(Editor::Window) {
        if args.prompt.is_some() || args.timeout.is_some() {
            problems.push(
                "--editor window cannot take --prompt or --timeout: only the overlay asks and times out"
                    .to_owned(),
            );
        }
        if args.output.as_deref() == Some(Path::new("-")) {
            problems.push(
                "--editor window cannot write to -o -: the window outlives this terminal's pipe"
                    .to_owned(),
            );
        }
    }
    problems.extend(stdio(
        args.output.as_deref(),
        args.annotate.as_deref(),
        args.json,
        terminal,
    ));
    joined(problems)
}

pub fn validate_capture(args: &CaptureArgs, terminal: Terminal) -> Result<(), String> {
    let mut problems = Vec::new();
    if let Err(error) = args.target.resolve(true) {
        problems.push(error);
    }
    problems.extend(stdio(
        args.output.as_deref(),
        args.annotate.as_deref(),
        args.json,
        terminal,
    ));
    joined(problems)
}

pub fn validate_annotate(args: &AnnotateArgs, terminal: Terminal) -> Result<(), String> {
    let mut problems = stdio(
        args.output.as_deref(),
        args.annotate.as_deref(),
        args.json,
        terminal,
    );
    if args.image.is_none() && !args.pick {
        problems.push("give an IMAGE or --pick".to_owned());
    }
    if args.pick && args.headless {
        problems.push("--pick needs the editor; omit --headless".to_owned());
    }
    if args.headless {
        if args.annotate.is_none() {
            problems.push("--headless needs --annotate".to_owned());
        }
        if args.tool.is_some() || args.prompt.is_some() || args.timeout.is_some() {
            problems.push("tool, prompt and timeout need the editor; omit --headless".to_owned());
        }
    } else {
        if args.copy || args.no_save {
            problems
                .push("--copy and --no-save need headless annotation; add --headless".to_owned());
        }
        if args.output.as_deref() == Some(Path::new("-")) {
            problems.push("the editor cannot write to -o -; add --headless".to_owned());
        }
    }
    if args.no_save && (!args.copy || args.output.is_some()) {
        problems.push("--no-save needs --copy and conflicts with --output".to_owned());
    }
    if let Some(image) = &args.image
        && let Err(error) = crate::finish::check_output(image, args.output.as_deref())
    {
        problems.push(error.to_string());
    }
    joined(problems)
}

fn concrete(target: Option<&Target>) -> bool {
    matches!(
        target,
        Some(
            Target::CurrentScreen
                | Target::Screen(_)
                | Target::FocusedWindow
                | Target::Window(_)
                | Target::Region(_)
        )
    )
}

fn stdio(
    output: Option<&Path>,
    annotate: Option<&Path>,
    json: bool,
    terminal: Terminal,
) -> Vec<String> {
    let mut problems = Vec::new();
    let to_stdout = output == Some(Path::new("-"));
    if to_stdout && json {
        problems.push("-o - and --json both want stdout".to_owned());
    }
    if to_stdout && terminal.stdout {
        problems.push("refusing to write PNG to a terminal; redirect or pass -o FILE".to_owned());
    }
    if annotate == Some(Path::new("-")) && terminal.stdin {
        problems.push(
            "--annotate - reads stdin, which is a terminal; it would wait forever".to_owned(),
        );
    }
    problems
}

fn joined(problems: Vec<String>) -> Result<(), String> {
    match problems.is_empty() {
        true => Ok(()),
        false => Err(problems.join("; ")),
    }
}

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Cli, clap::Error> {
    parse_in(args, true)
}

pub fn parse_forwarded(args: impl IntoIterator<Item = OsString>) -> Result<Cli, clap::Error> {
    parse_in(args, false)
}

fn parse_in(args: impl IntoIterator<Item = OsString>, own_env: bool) -> Result<Cli, clap::Error> {
    let args = attach_negative_geometry(args);
    let mut command = Cli::command();
    let matches = match command.try_get_matches_from_mut(args.clone()) {
        Ok(matches) => matches,
        Err(error) => return Err(editor_option_after(&mut command, &args, error)),
    };
    if let Some((name, _)) = matches.subcommand() {
        let editor_only = command
            .get_arguments()
            .find(|arg| {
                !arg.is_global_set()
                    && matches.value_source(arg.get_id().as_str()) == Some(ValueSource::CommandLine)
            })
            .and_then(|arg| arg.get_long().map(str::to_owned));
        if let Some(long) = editor_only {
            return Err(editor_only_error(&mut command, &long, name, &args));
        }
    }
    let mut cli = Cli::from_arg_matches(&matches)?;
    if !own_env && matches.value_source("config") == Some(ValueSource::EnvVariable) {
        cli.config.config = None;
    }
    if let Some(file) = &cli.gui.file
        && !file
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
    {
        let message = format!(
            "{} is neither a subcommand nor a .png file to open",
            file.display()
        );
        return Err(command.error(clap::error::ErrorKind::InvalidSubcommand, message));
    }
    Ok(cli)
}

fn editor_option_after(
    command: &mut clap::Command,
    args: &[OsString],
    error: clap::Error,
) -> clap::Error {
    if args.iter().any(|arg| arg == "annotate") {
        let has = |flag: &str| args.iter().any(|arg| arg == flag);
        let hint = if !has("--headless") && (has("--copy") || has("--no-save")) {
            Some("add --headless")
        } else if has("--headless")
            && ["--tool", "--prompt", "--timeout"].iter().any(|flag| {
                args.iter().any(|arg| {
                    arg == flag || arg.to_string_lossy().starts_with(&format!("{flag}="))
                })
            })
        {
            Some("omit --headless")
        } else {
            None
        };
        if let Some(hint) = hint {
            let text = error.to_string();
            let reason = text
                .split("\n\n")
                .next()
                .unwrap_or_default()
                .trim_start_matches("error: ");
            let message = format!("{reason}\n{hint}");
            return match command.find_subcommand_mut("annotate") {
                Some(annotate) => annotate.error(error.kind(), message),
                None => command.error(error.kind(), message),
            };
        }
        return error;
    }
    if error.kind() != clap::error::ErrorKind::UnknownArgument {
        return error;
    }
    let Some(clap::error::ContextValue::String(given)) =
        error.get(clap::error::ContextKind::InvalidArg)
    else {
        return error;
    };
    let long = given.trim_start_matches('-');
    let long = long
        .split_once('=')
        .map_or(long, |(name, _)| name)
        .to_owned();
    let editor_only = command
        .get_arguments()
        .any(|arg| !arg.is_global_set() && arg.get_long() == Some(long.as_str()));
    let subcommand = args.iter().skip(1).find_map(|arg| {
        let arg = arg.to_str()?;
        command
            .get_subcommands()
            .find(|sub| sub.get_name() == arg)
            .map(|sub| sub.get_name().to_owned())
    });
    match (editor_only, subcommand) {
        (true, Some(name)) => editor_only_error(command, &long, &name, args),
        _ => error,
    }
}

fn editor_only_error(
    command: &mut clap::Command,
    long: &str,
    name: &str,
    args: &[OsString],
) -> clap::Error {
    let mut rest: Vec<String> = args
        .iter()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .filter(|arg| arg != name)
        .collect();
    let targeted = rest.iter().any(|arg| {
        ["--screen", "--window", "--region"]
            .iter()
            .any(|target| arg == target || arg.starts_with(&format!("{target}=")))
    });
    if !targeted {
        rest.insert(0, "--screen".to_owned());
    }
    let message = format!(
        "--{long} is an editor option; `{name}` never opens a window. Run `glimpse-shot {}` instead.",
        rest.join(" ")
    );
    command.error(clap::error::ErrorKind::ArgumentConflict, message)
}

pub fn attach_negative_geometry(args: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    let mut attached: Vec<OsString> = Vec::new();
    for arg in args {
        let negative = arg.to_str().is_some_and(|text| {
            text.strip_prefix('-')
                .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
        });
        match attached.last_mut() {
            Some(last) if negative && last == "--region" => {
                last.push("=");
                last.push(&arg);
            }
            _ => attached.push(arg),
        }
    }
    attached
}

pub fn missing_directory(output: Option<&Path>) -> Option<PathBuf> {
    let output = output.filter(|path| *path != Path::new("-"))?;
    let parent = match output.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    (!parent.is_dir()).then(|| parent.to_path_buf())
}

#[cfg(test)]
mod tests;
