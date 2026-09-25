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
            Some(Command::List { json }) => *json,
            Some(Command::Prime | Command::Portal | Command::Hold) => false,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Take a screenshot without any window or overlay")]
    Capture(CaptureArgs),
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
        default_missing_value = "",
        value_name = "ID",
        help = "A window: picked by hand, the focused one, or the one with this id."
    )]
    pub window: Option<String>,

    #[arg(
        long,
        num_args = 0..=1,
        default_missing_value = "",
        value_name = "GEOMETRY",
        help = "A rectangle as \"X,Y WxH\" in the compositor's logical layout, or dragged by hand."
    )]
    pub region: Option<String>,
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
                name => Ok(Target::Screen(name.to_owned())),
            };
        }
        if let Some(window) = &self.window {
            return match window.as_str() {
                "" if headless => Ok(Target::FocusedWindow),
                "" => Ok(Target::PickWindow),
                "focused" => Ok(Target::FocusedWindow),
                id => id
                    .parse()
                    .map(Target::Window)
                    .map_err(|_| format!("--window takes a window id or \"focused\", got {id:?}")),
            };
        }
        if let Some(region) = &self.region {
            return match region.as_str() {
                "" if headless => Err("--region needs a geometry such as \"X,Y WxH\"".to_owned()),
                "" => Ok(Target::PickRegion),
                geometry => parse_geometry(geometry)
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
}

#[derive(Debug, Clone, Default, Args)]
#[command(after_help = AGENT_POINTER, after_long_help = AGENT_POINTER)]
pub struct CaptureArgs {
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

fn paired(yes: bool, no: bool) -> Option<bool> {
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
    let targeted = args.target.is_set();
    if let Err(error) = &target {
        problems.push(error.clone());
    }
    if args.annotate.is_some() && !concrete(target.as_ref().ok()) {
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
mod tests {
    use clap::CommandFactory;

    use super::*;
    use crate::errors::Exit;
    use glimpse_config::PaletteColor;
    use glimpse_shot::doc::units::{Logical, Rect};
    use glimpse_shot::doc::wire::{self, AnnotationV1, MAX_MARKS, MAX_PATH_POINTS, MAX_TEXT_CHARS};

    const PIPES: Terminal = Terminal {
        stdin: false,
        stdout: false,
    };
    const TTY: Terminal = Terminal {
        stdin: true,
        stdout: true,
    };

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        super::parse(
            std::iter::once("glimpse-shot")
                .chain(args.iter().copied())
                .map(OsString::from),
        )
    }

    #[test]
    fn a_negative_geometry_is_attached_to_region_and_nothing_else() {
        let args = |list: &[&str]| {
            attach_negative_geometry(list.iter().map(OsString::from))
                .into_iter()
                .map(|arg| arg.into_string().expect("utf-8"))
                .collect::<Vec<_>>()
        };

        assert_eq!(
            args(&["capture", "--region", "-100,50 400x300", "-o", "-"]),
            ["capture", "--region=-100,50 400x300", "-o", "-"]
        );
        assert_eq!(
            args(&["--region", "-o", "x.png"]),
            ["--region", "-o", "x.png"]
        );
        assert_eq!(args(&["-o", "-5.png"]), ["-o", "-5.png"]);
    }

    fn clap_code(args: &[&str]) -> i32 {
        parse(args).expect_err("a usage error").exit_code()
    }

    fn capture(args: &[&str]) -> CaptureArgs {
        match parse(&[&["capture"], args].concat())
            .expect("parses")
            .command
        {
            Some(Command::Capture(capture)) => capture,
            other => panic!("expected capture, got {other:?}"),
        }
    }

    fn gui(args: &[&str]) -> GuiArgs {
        let cli = parse(args).expect("parses");
        assert!(cli.command.is_none());
        cli.gui
    }

    #[test]
    fn every_tool_is_a_starting_tool_and_an_unknown_one_lists_them() {
        for (name, tool) in [
            ("crop", Tool::Crop),
            ("arrow", Tool::Arrow),
            ("line", Tool::Line),
            ("freehand", Tool::Freehand),
            ("highlighter", Tool::Highlighter),
            ("rect", Tool::Rect),
            ("filled-rect", Tool::FilledRect),
            ("oval", Tool::Oval),
            ("number", Tool::Number),
            ("text", Tool::Text),
            ("blur", Tool::Blur),
        ] {
            assert_eq!(gui(&["--tool", name]).tool, Some(tool), "{name}");
        }
        assert_eq!(gui(&["--window", "--tool", "crop"]).tool, Some(Tool::Crop));
        assert_eq!(gui(&[]).tool, None);
        let refused = parse(&["--tool", "pen"]).expect_err("refused");
        assert_eq!(refused.exit_code(), 2);
        let message = refused.to_string();
        for name in [
            "crop",
            "arrow",
            "line",
            "freehand",
            "highlighter",
            "filled-rect",
            "blur",
        ] {
            assert!(message.contains(name), "{message}");
        }
    }

    #[test]
    fn the_argument_surface_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn prime_prints_the_embedded_guide_without_a_session() {
        let mut output = Vec::new();
        prime::print(&mut output).expect("prime succeeds");
        assert_eq!(output, prime::GUIDE.as_bytes());
        assert!(matches!(
            parse(&["prime"]).expect("parses").command,
            Some(Command::Prime)
        ));
    }

    #[test]
    fn prime_names_every_wire_kind_limit_color_and_exit_code() {
        let samples = [
            AnnotationV1::Arrow {
                from: [0, 0],
                to: [1, 1],
                color: PaletteColor::Red,
                stroke: 2,
            },
            AnnotationV1::Line {
                from: [0, 0],
                to: [1, 1],
                color: PaletteColor::Red,
                stroke: 2,
            },
            AnnotationV1::Freehand {
                points: vec![[0, 0], [1, 1]],
                color: PaletteColor::Red,
                stroke: 2,
            },
            AnnotationV1::Highlighter {
                points: vec![[0, 0], [1, 1]],
                color: PaletteColor::Red,
                stroke: 2,
            },
            AnnotationV1::Rect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                color: PaletteColor::Red,
                stroke: 2,
                filled: false,
            },
            AnnotationV1::Oval {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                color: PaletteColor::Red,
                stroke: 2,
            },
            AnnotationV1::Number {
                x: 0,
                y: 0,
                color: PaletteColor::Red,
                stroke: 2,
                n: 1,
            },
            AnnotationV1::Text {
                x: 0,
                y: 0,
                text: String::new(),
                color: PaletteColor::Red,
                stroke: 2,
            },
            AnnotationV1::Blur {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            AnnotationV1::Pixelate {
                points: vec![[0, 0], [1, 1]],
                width: 32,
            },
        ];
        let count = samples.len();
        let mut kinds = std::collections::HashSet::new();
        for sample in samples {
            match &sample {
                AnnotationV1::Arrow { .. }
                | AnnotationV1::Line { .. }
                | AnnotationV1::Freehand { .. }
                | AnnotationV1::Highlighter { .. }
                | AnnotationV1::Rect { .. }
                | AnnotationV1::Oval { .. }
                | AnnotationV1::Number { .. }
                | AnnotationV1::Text { .. }
                | AnnotationV1::Blur { .. }
                | AnnotationV1::Pixelate { .. } => {}
            }
            let value = serde_json::to_value(sample).expect("serializes");
            let kind = value["kind"].as_str().expect("kind");
            assert!(
                prime::GUIDE.contains(&format!("`{kind}`")),
                "missing {kind}"
            );
            kinds.insert(kind.to_owned());
        }
        assert_eq!(kinds.len(), count);
        for limit in [MAX_MARKS, MAX_PATH_POINTS, MAX_TEXT_CHARS] {
            assert!(prime::GUIDE.contains(&limit.to_string()), "missing {limit}");
        }
        for color in ["red", "yellow", "green", "blue", "white"] {
            assert!(
                prime::GUIDE.contains(&format!("`{color}`")),
                "missing {color}"
            );
        }
        for code in [
            Exit::Ok,
            Exit::Failed,
            Exit::Usage,
            Exit::Config,
            Exit::Cancelled,
            Exit::NotFound,
            Exit::TimedOut,
        ] {
            match code {
                Exit::Ok
                | Exit::Failed
                | Exit::Usage
                | Exit::Config
                | Exit::Cancelled
                | Exit::NotFound
                | Exit::TimedOut => {}
            }
            assert!(
                prime::GUIDE.contains(&format!("| {} |", code as u8)),
                "missing exit code {}",
                code as u8
            );
        }
    }

    #[test]
    fn prime_annotation_json_examples_parse_and_validate() {
        let mut count = 0;
        for fence in prime::GUIDE.split("```json").skip(1) {
            let example = fence.split("```").next().expect("closed fence").trim();
            if example.starts_with('[') {
                let annotations: Vec<AnnotationV1> =
                    serde_json::from_str(example).expect("annotation JSON");
                wire::marks(&annotations).expect("valid annotations");
                count += 1;
            }
        }
        assert!(count > 0, "no annotation JSON example");

        let heredoc = prime::GUIDE
            .split("<<'EOF'\n")
            .nth(1)
            .expect("annotation heredoc")
            .split("\nEOF")
            .next()
            .expect("heredoc end");
        let annotations: Vec<AnnotationV1> = serde_json::from_str(heredoc).expect("heredoc JSON");
        wire::marks(&annotations).expect("valid heredoc annotations");
    }

    #[test]
    fn short_and_long_help_point_to_prime_without_a_field_list() {
        for args in [
            &["-h"][..],
            &["--help"][..],
            &["capture", "-h"][..],
            &["capture", "--help"][..],
        ] {
            let help = parse(args).expect_err("help exits").to_string();
            assert!(help.trim_end().ends_with(AGENT_POINTER), "{help}");
            assert!(!help.contains("from: [x, y]"), "{help}");
        }
    }

    #[test]
    fn the_hidden_service_flag_is_accepted_and_handed_to_gtk_alone() {
        let cli = parse(&["--gapplication-service"]).expect("parses");
        assert!(cli.command.is_none());
        assert!(cli.gui.gapplication_service);
        assert!(validate_gui(&cli.gui, PIPES).is_ok());
        let help = parse(&["--help"]).expect_err("help exits").to_string();
        assert!(!help.contains("gapplication"), "{help}");

        let argv = ["/usr/bin/glimpse-shot", "--gapplication-service"].map(OsString::from);
        assert_eq!(
            crate::gui::app::gtk_args(&cli.gui, argv.to_vec()),
            argv.to_vec()
        );
        let forwarded = ["glimpse-shot", "--screen", "--json"].map(OsString::from);
        assert_eq!(
            crate::gui::app::gtk_args(&gui(&["--screen", "--json"]), forwarded.to_vec()),
            forwarded.to_vec(),
            "a GUI invocation is forwarded whole to the primary instance"
        );
    }

    #[test]
    fn clap_rows_exit_two() {
        assert_eq!(
            clap_code(&["capture", "--no-save", "--copy", "-o", "x.png"]),
            2
        );
        assert_eq!(clap_code(&["capture", "--no-save"]), 2);
        assert_eq!(clap_code(&["capture", "--screen", "--window"]), 2);
        assert_eq!(clap_code(&["--screen", "DP-2", "--region", "0,0 1x1"]), 2);
        assert_eq!(clap_code(&["shot.png", "--screen"]), 2);
        assert_eq!(clap_code(&["--window", "42", "shot.png"]), 2);
        assert_eq!(clap_code(&["--region", "0,0 1x1", "shot.png"]), 2);
        for flag in [
            "--flash",
            "--no-flash",
            "--notify",
            "--no-notify",
            "--editor=overlay",
            "--prompt=x",
            "--timeout=5",
        ] {
            assert_eq!(clap_code(&["capture", flag]), 2, "{flag}");
        }
        assert_eq!(clap_code(&["capture", "--delay", "61"]), 2);
        assert_eq!(clap_code(&["--delay", "61"]), 2);
    }

    #[test]
    fn global_options_go_before_or_after_the_subcommand() {
        for args in [
            &["--config", "/x.toml", "capture", "--screen", "DP-9"][..],
            &["capture", "--config", "/x.toml", "--screen", "DP-9"][..],
            &[
                "-c", "/x.toml", "--log", "debug", "--color", "never", "capture", "--screen",
                "DP-9",
            ][..],
        ] {
            let cli = parse(args).unwrap_or_else(|error| panic!("{args:?}: {error}"));
            assert_eq!(
                cli.config.as_deref(),
                Some(Path::new("/x.toml")),
                "{args:?}"
            );
            match cli.command {
                Some(Command::Capture(capture)) => {
                    assert_eq!(capture.target.screen.as_deref(), Some("DP-9"))
                }
                other => panic!("{args:?}: expected capture, got {other:?}"),
            }
        }

        let cli = parse(&["--log", "debug", "list", "--json"]).expect("parses");
        assert_eq!(cli.log.log, "debug");
        assert!(matches!(cli.command, Some(Command::List { json: true })));

        let cli = parse(&["--log-format", "json", "hold"]).expect("parses");
        assert!(matches!(cli.command, Some(Command::Hold)));
    }

    #[test]
    fn without_a_subcommand_the_default_command_still_takes_its_file_and_target() {
        let cli = parse(&["--config", "/x.toml", "shot.png"]).expect("parses");
        assert!(cli.command.is_none());
        assert_eq!(cli.gui.file.as_deref(), Some(Path::new("shot.png")));

        let gui = gui(&["--screen", "--json"]);
        assert_eq!(gui.target.screen.as_deref(), Some("current"));
        assert!(gui.json);
    }

    #[test]
    fn a_bare_word_is_a_subcommand_or_a_png_and_nothing_else() {
        assert_eq!(clap_code(&["captur"]), 2);
        assert_eq!(clap_code(&["notes.txt"]), 2);
        let cli = parse(&["Shot.PNG"]).expect("parses");
        assert!(cli.command.is_none());
        assert_eq!(cli.gui.file.as_deref(), Some(Path::new("Shot.PNG")));
    }

    #[test]
    fn a_default_command_option_before_a_subcommand_is_refused() {
        assert_eq!(clap_code(&["--json", "capture"]), 2);
        assert_eq!(clap_code(&["--screen", "DP-2", "list"]), 2);
        assert_eq!(clap_code(&["-o", "x.png", "capture"]), 2);
    }

    #[test]
    fn an_editor_only_option_given_to_capture_says_why_and_what_to_run() {
        let command = Cli::command();
        let capture = command.find_subcommand("capture").expect("capture exists");
        let own: Vec<&str> = capture
            .get_arguments()
            .filter_map(|arg| arg.get_long())
            .collect();
        let editor_only: Vec<(String, bool)> = command
            .get_arguments()
            .filter(|arg| !arg.is_global_set() && !arg.is_hide_set())
            .filter_map(|arg| {
                let long = arg.get_long()?;
                (!own.contains(&long)).then(|| (long.to_owned(), arg.get_action().takes_values()))
            })
            .collect();
        for flag in ["editor", "prompt", "timeout", "flash", "no-flash", "tool"] {
            assert!(
                editor_only.iter().any(|(long, _)| long == flag),
                "--{flag} is not editor-only: {editor_only:?}"
            );
        }
        for (long, takes_value) in &editor_only {
            let flag = format!("--{long}");
            let mut args = vec!["capture", flag.as_str()];
            if *takes_value {
                args.push("window");
            }
            let refused = parse(&args).expect_err("refused");
            assert_eq!(refused.exit_code(), 2, "{flag}");
            let message = refused.to_string();
            assert!(
                message.contains(&format!(
                    "{flag} is an editor option; `capture` never opens a window"
                )),
                "{message}"
            );
            assert!(message.contains("Run `glimpse-shot --screen"), "{message}");
        }
        let message = parse(&["capture", "--window", "--editor", "window"])
            .expect_err("refused")
            .to_string();
        assert!(
            message.contains("Run `glimpse-shot --window --editor window` instead."),
            "{message}"
        );
    }

    #[test]
    fn the_last_of_a_pair_of_flags_wins() {
        assert!(!capture(&["--cursor", "--no-cursor"]).cursor(true));
        assert!(capture(&["--no-cursor", "--cursor"]).cursor(false));
        assert!(capture(&[]).cursor(true));
        assert!(!capture(&[]).cursor(false));
    }

    #[test]
    fn capture_takes_its_own_options() {
        let args = capture(&[
            "--region",
            "-100,50 400x300",
            "-o",
            "-",
            "--scale",
            "logical",
            "-d",
            "3",
        ]);

        assert_eq!(args.output.as_deref(), Some(Path::new("-")));
        assert_eq!(args.scale, ScaleMode::Logical);
        assert_eq!(args.delay, Some(3));
        assert_eq!(
            args.target.resolve(true),
            Ok(Target::Region(Rect {
                x: Logical(-100.0),
                y: Logical(50.0),
                width: Logical(400.0),
                height: Logical(300.0),
            }))
        );
    }

    #[test]
    fn targets_resolve_per_the_targets_table() {
        let resolve = |args: &[&str], headless: bool| {
            if headless {
                capture(args).target.resolve(true)
            } else {
                gui(args).target.resolve(false)
            }
        };

        assert_eq!(resolve(&[], false), Ok(Target::App));
        assert_eq!(resolve(&[], true), Ok(Target::CurrentScreen));
        assert_eq!(resolve(&["--screen"], true), Ok(Target::CurrentScreen));
        assert_eq!(
            resolve(&["--screen", "current"], false),
            Ok(Target::CurrentScreen)
        );
        assert_eq!(
            resolve(&["--screen", "DP-2"], true),
            Ok(Target::Screen("DP-2".to_owned()))
        );
        assert!(resolve(&["--screen", "all"], true).is_err());
        assert_eq!(resolve(&["--window"], false), Ok(Target::PickWindow));
        assert_eq!(resolve(&["--window"], true), Ok(Target::FocusedWindow));
        assert_eq!(
            resolve(&["--window", "focused"], false),
            Ok(Target::FocusedWindow)
        );
        assert_eq!(resolve(&["--window", "42"], true), Ok(Target::Window(42)));
        assert!(resolve(&["--window", "firefox"], true).is_err());
        assert_eq!(resolve(&["--region"], false), Ok(Target::PickRegion));
        assert!(resolve(&["--region"], true).is_err());
        assert!(resolve(&["--region", "10,10"], true).is_err());
    }

    #[test]
    fn usage_rows_are_refused_before_any_capture() {
        let refused = |args: CaptureArgs, terminal| validate_capture(&args, terminal).is_err();

        assert!(refused(capture(&["-o", "-", "--json"]), PIPES));
        assert!(refused(capture(&["-o", "-"]), TTY));
        assert!(!refused(capture(&["-o", "-"]), PIPES));
        assert!(refused(capture(&["--annotate", "-"]), TTY));
        assert!(!refused(capture(&["--annotate", "-"]), PIPES));
        assert!(refused(capture(&["--region"]), PIPES));
        assert!(refused(capture(&["--region", "garbage"]), PIPES));
        assert!(!refused(capture(&["--region", "0,0 10x10", "--json"]), TTY));

        let refused = |args: GuiArgs| validate_gui(&args, PIPES).is_err();
        assert!(refused(gui(&["--annotate", "marks.json"])));
        assert!(refused(gui(&["--window", "--annotate", "marks.json"])));
        assert!(refused(gui(&["--region", "--annotate", "marks.json"])));
        assert!(!refused(gui(&[
            "--window",
            "42",
            "--annotate",
            "marks.json"
        ])));
        assert!(refused(gui(&["--json"])));
        assert!(refused(gui(&["--prompt", "where is the bug?"])));
        assert!(refused(gui(&["--timeout", "30"])));
        assert!(!refused(gui(&[
            "--screen",
            "--json",
            "--prompt",
            "x",
            "--timeout",
            "30"
        ])));

        let window = |extra: &[&str]| {
            let mut args = vec!["--screen", "--editor", "window"];
            args.extend_from_slice(extra);
            validate_gui(&gui(&args), PIPES)
        };
        assert!(window(&[]).is_ok());
        assert!(window(&["-o", "shot.png"]).is_ok());
        for conflict in [&["--prompt", "x"][..], &["--timeout", "30"], &["-o", "-"]] {
            let problem = window(conflict).expect_err("refused");
            assert!(problem.contains("--editor window"), "{problem}");
        }
        assert!(validate_gui(&gui(&["--screen", "--editor", "overlay", "-o", "-"]), PIPES).is_ok());
    }

    #[test]
    fn every_problem_is_reported_at_once() {
        let problems = validate_capture(&capture(&["--region", "-o", "-", "--json"]), TTY)
            .expect_err("three problems");

        assert!(problems.contains("--region needs a geometry"));
        assert!(problems.contains("both want stdout"));
        assert!(problems.contains("refusing to write PNG to a terminal"));
    }

    #[test]
    fn only_an_explicit_output_directory_must_exist() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let present = dir.path().join("shot.png");
        let absent = dir.path().join("missing").join("shot.png");

        assert_eq!(missing_directory(None), None);
        assert_eq!(missing_directory(Some(Path::new("-"))), None);
        assert_eq!(missing_directory(Some(Path::new("shot.png"))), None);
        assert_eq!(missing_directory(Some(&present)), None);
        assert_eq!(
            missing_directory(Some(&absent)),
            Some(dir.path().join("missing"))
        );
    }
}
