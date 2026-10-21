# Config split, unified discovery and one watch per process

Status: approved in a grilling session with the user, 2026-10-21. No backward compatibility.

## Goal

`config.toml` keeps every table except the panel layout, which moves to `panel.toml` + `panel.d/`.
Discovery becomes one root stack shared by config, themes, stylesheets and wallpaper images. Each
process runs one watch over its writable roots and every reader subscribes to it. Config and theme
reloads share one result type.

## Current state (checked against the tree, master `faba30bf`)

| Concern | Where | Shape today |
| --- | --- | --- |
| Config layers | `glimpse-config/src/load.rs` `stack()`, `load_from()` | `/etc/glimpse/config.toml`, `config.d/`, user `config.toml`, user `config.d/`, `shot.toml` (`[screenshot]` only, filtered at `load_from`) |
| Override | `glimpse-utils/src/args.rs:30-49` `ConfigArg` | `--config <file>` / `GLIMPSE_CONFIG_PATH`, replaces layers 2-5, threaded as `config_path` (129 references outside `glimpse-config`) |
| Theme roots | `glimpse-config/src/theme.rs:47` `theme_dirs()` | user `themes/`, `DATA_DIR/themes`; ignores `--config` |
| Image roots | `load.rs` `resolve_in()` | user dir, `DATA_DIR/wallpapers` |
| Watch core | `glimpse-config/src/watch.rs` `Watch` | one `notify::RecommendedWatcher` per call; notify 8.2.0 spawns one `notify-rs inotify loop` thread each (`notify-8.2.0/src/inotify.rs:123`, measured) |
| Config watch edges | panel `app.rs:684`, lock `app.rs:942`, notifications `app.rs:1043`, wallpaper `app.rs:178`; idle, sunset, settings-daemon, weather `main.rs` | each matches `Result<Option<Config>, String>` itself |
| Theme watch edges | panel `app.rs:674`, lock `app.rs:932`, notifications `app.rs:1061` (`watch_styles`), wallpaper `app.rs:168` | four identical functions; GTK thread resolves and reads CSS (`panel/app.rs:487-497`, `widgets/src/theme.rs:282`) |
| Writers | wallpaper `portal.rs:110-160`, shot `gui/prefs.rs:33,80` | wallpaper checks `shadowing_dropin`; shot writes `shot.toml` |
| Seed | `load.rs` `seed_user_config`, `seed_from` | copies `DATA_DIR/config.commented.toml` to user `config.toml` |
| Services | 17 `impl From<&Config>` in `glimpse-services`; exec reads `[[panels]]`/`[applets]` via `placed_applets(document)` (`exec/mod.rs:107`) | whole-document projection |
| Fifth stylesheet reader | shot `gui/app.rs:122-125` | builds `Sheets` once, no watch |
| Fourth root copy | compositors `keyboard.rs:46-53` `search_paths()` | user dir, then `DATA_DIR` |
| Dev panel | glimpsectl `commands/dev.rs:92-125` | `Config::default()` unless `--config` was given explicitly (then that file), edits its panels, serializes it to a scratch `config.toml`, starts the panel with `--config` |
| Shot children | shot `portal.rs:159-166` `headless_command`, `adapters/clipboard.rs:50-52` | forward `--config <path>` to child processes |
| Single-file validate | glimpsectl `cli.rs:480-482`, `main.rs:154` | `config validate [PATH]` loads one file |
| Install seeding | `scripts/install.sh:86-128`, pinned by `glimpse-package/tests/packaging.rs:605-631,689-695` | copies `config.commented.toml` to the user's `config.toml` |
| Packaging and CI | `.github/workflows/config-drift.yml:26-30`, `glimpse-package/Cargo.toml:69-71,152-154,224-226`, `scripts/package-binary.sh:48-50`, `nix/package.nix:40-41,75-80`, `website/.vitepress/schema.data.ts:5,107` | name the three `config.*` reference files; the website reads `[applets]` from `config.schema.json` |
| Flag handling | glimpsectl `cli.rs:954-958` (`value_source("config")`), `cli.rs:994` (`"--config" \| "-c"` in `command_index`) | knows the flag by name |
| Remote invocation | shot `gui/app.rs:40` `CONFIG_VARIABLE`, `:295-304`, `caller_config` `:1121-1131` | each invocation of the single instance brings its own config path |

## Decisions

1. **One `Config` struct, two files.** `[[panels]]` and `[applets]` live in `panel.toml` and
   `panel.d/*.toml`; every other table, `[screenshot]` included, lives in `config.toml`. The panel
   and `glimpsectl` load both stacks into one `Config`; every other binary loads `config.toml`
   only, so a broken `panel.toml` never sends a daemon to defaults. Services, the `S::Config`
   bound and the exec service are untouched.
2. **Each file owns its tables.** `[[panels]]`/`[applets]` in a `config.toml` stack file, or any
   other table in a `panel.toml`/`panel.d` file, fails the load with a schema error naming the
   owning file. A `config.toml` still holding `[[panels]]` therefore fails to load in every
   binary until the user moves those tables; `website/start/upgrade.md` says so.
3. **Root stack, highest first:** `--config-dir` / `GLIMPSE_CONFIG_DIR`, then
   `$XDG_CONFIG_HOME/glimpse`, then `/etc/glimpse`. Config merges every root lowest first. Themes,
   `styles.css`, `dark.css` and wallpaper images take the first hit, with `DATA_DIR` appended as the
   built-in assets root. `DATA_DIR` is never a config layer.
4. **Drop-ins only for the panel:** `panel.d/` in every root, read after that root's `panel.toml`.
5. **Writes go to the top root** (`--config-dir` when given, else the user dir); with neither,
   there is no writable file and `writable_file` returns `None` — never `/etc/glimpse`.
6. **One watch per process** over the writable roots (top root and user dir, never `/etc`), plus
   the existing symlink-target parents, plus `panel.d/`, `themes/` and every directory directly
   under `themes/` in each writable root. The set is a function recomputed after every event, as
   `watch.rs:62-69` already does, so a theme directory created later joins it. When
   `GLIMPSE_THEMES_DIR` is set, that directory and its immediate subdirectories join the set too.
   One SIGHUP stream. Readers subscribe through a broadcast; each rereads on every
   event and drops results equal to what it holds. Service watchers (calendar, places, exec
   applications) stay on `watch_dirs` with their own watch.
7. **`Reload<T>` for config and theme.** `Theme` carries stylesheet contents read off the GTK
   thread; `Rejected` is an unreadable file, never a CSS syntax error (GTK's `parsing-error` keeps
   that). CSS loads with `load_from_string`, so a user theme's relative `url()`/`@import` no longer
   resolves; the shipped themes use none, and `website/guide/theming.md` says so.
8. **Isolation is the environment's job.** Tests set `--config-dir` and a temporary
   `XDG_CONFIG_HOME`; the finder has no test mode. `/etc/glimpse` cannot be isolated this way;
   that leak into integration tests is accepted. Unit tests never reach `roots()`: every API that
   reads or writes takes explicit paths internally, and `load_files(&[PathBuf])` loads a given list.
9. **Seed a stub**, never the commented document: `config.toml` and `panel.toml` each get a
   `#:schema` line and a link to the docs, only when the file is absent and no `--config-dir` was
   given; the user directory is created if missing. A stub `panel.toml` sets nothing, so the
   default layout applies. `scripts/install.sh` stops seeding; the binaries do it on first start.
10. **No backward compatibility.** Deleted outright: `config.d/`, `shot.toml`, `--config`,
    `GLIMPSE_CONFIG_PATH`, `shadowing_dropin`, `retired_power`, `retired_auto_dark`, the
    `color-scheme = "auto"` guidance. A leftover file is simply not read.
11. **Every root copy uses the stack.** `find()` replaces `theme_dirs()`, `user_sheet_in()`,
    `resolve_in()`'s root walk and compositors `keyboard.rs` `search_paths()`.
    `GLIMPSE_THEMES_DIR` and `GLIMPSE_THEME` keep overriding theme lookup as they do today.
12. **Shot's single instance serves every invocation from its own roots.** `CONFIG_VARIABLE` and
    `caller_config` are deleted; a remote invocation's `--config-dir` is not honored. Child
    processes shot spawns (`headless_command`, the clipboard `hold`) get `--config-dir <dir>` when
    `config_dir()` is `Some`, and nothing otherwise.
13. **`glimpsectl dev panel` stays hermetic.** Its base document is `Config::default()`, or
    `load(ConfigAndPanel)` when `--config-dir` was given explicitly. It edits the panels, writes the
    document split by `PANEL_TABLES` into `<scratch>/config.toml` and `<scratch>/panel.toml`, and
    starts the panel with `--config-dir <scratch>` and `XDG_CONFIG_HOME=<scratch>/home`.
14. **No short flag.** `--config-dir` has none; `-c` leaves `command_index`.
15. **`glimpsectl config validate [PATH]` keeps its PATH**, loaded through `load_files`, which applies
    ownership by file name (`panel.toml` and files under a `panel.d/` own the panel tables).
16. **Theme problems are their own alert.** The panel's `Problems` gains a `theme` slot beside the
    config failure, so neither clears the other.

## Types and API

```rust
pub enum Documents { Config, ConfigAndPanel }

pub enum Reload<T> { Changed(T), Recovered, Rejected(String) }

#[derive(Clone, PartialEq)]
pub struct Theme {
    pub sheet: Option<String>,
    pub dark: Option<String>,
    pub user: Option<String>,
    pub user_dark: Option<String>,
}

pub fn init(config_dir: Option<PathBuf>);
pub fn config_dir() -> Option<&'static Path>;
pub fn roots() -> Vec<PathBuf>;
pub fn config_files(documents: Documents) -> Result<Vec<PathBuf>, ConfigError>;
pub fn find(relative: &Path) -> Option<PathBuf>;
pub fn writable_file(name: &str) -> Option<PathBuf>;

pub fn load(documents: Documents) -> Result<Config, ConfigError>;
pub fn load_files(files: &[PathBuf]) -> Result<Config, ConfigError>;
pub fn load_or_default(documents: Documents) -> (Config, Option<ConfigError>);
pub fn watch_config(documents: Documents, current: Config, failure: Option<String>)
    -> impl Stream<Item = Reload<Config>> + Send + 'static;
pub fn load_theme(name: &str, sheet: &str) -> Result<Theme, ConfigError>;
pub fn watch_theme(name: &str, sheet: &'static str, current: Theme)
    -> impl Stream<Item = Reload<Theme>> + Send + 'static;
pub fn seed_user_config();
pub fn watch_dirs(dirs: impl IntoIterator<Item = PathBuf>)
    -> impl Stream<Item = Update> + Send + 'static;
```

`ConfigArg` in `glimpse-utils/src/args.rs` becomes:

```rust
pub struct ConfigArg {
    #[arg(long, global = true, env = "GLIMPSE_CONFIG_DIR", value_name = "DIR",
          help = "Directory stacked above the user configuration.")]
    pub config_dir: Option<PathBuf>,
}
```

Seed stubs, `{DATA_DIR}` substituted at build time:

```toml
#:schema {DATA_DIR}/config.schema.json
# https://alex-oleshkevich.github.io/glimpse/config/
```

```toml
#:schema {DATA_DIR}/panel.schema.json
# https://alex-oleshkevich.github.io/glimpse/config/panels
```

Generated references: `data/{config,panel}.{default.toml,commented.toml,schema.json}`, each
restricted to its file's tables; `scripts/install.sh:65-67` installs all six; the `justfile:65-75`
recipes take the document name.

## How the pieces connect

`main` calls `init(cli.config_dir)` before anything reads a path. `roots()` feeds
`config_files`, `find`, `writable_file` and the shared watch. `load(documents)` reads
`config_files(documents)` through the existing `read()` and `toml` merge, then checks table
ownership per file. `watch_config` and `watch_theme` both subscribe to the shared watch's
broadcast, start their reread on `spawn_blocking`, compare with the value they hold, and yield
`Reload`. UI binaries map `Reload<Config>` and `Reload<Theme>` to their inputs in one match each;
a theme rename restarts `watch_theme`, which re-subscribes and does one leading read, so the
new theme arrives without a file event. Startup reads the first `Theme` with `load_theme` before
the window maps, as `reload_styles` does today. Daemons match `Reload<Config>` in
their existing `select!` loop.

## Flows

**Startup, panel:** `init` → `load_or_default(ConfigAndPanel)` → seed stubs if absent → services
start from `Config` → `watch_config(ConfigAndPanel, …)` and `watch_theme(theme, PANEL_STYLESHEET)`
subscribe; the first subscriber starts the shared watch.

**Edit `panel.toml`:** the shared watch reports the change → every subscriber rereads → the theme
reader finds equal contents and yields nothing → the config reader yields `Changed(config)` →
the panel reconciles zones; services see equal slices and skip.

**Broken `panel.toml`:** the panel's reader yields `Rejected(reason)` once and keeps the running
config; daemons never read the file.

**`[lock]` written in `panel.toml`:** the load fails with an error naming `config.toml` as the
owner.

## Acceptance criteria

```
AC-1  As a user, I want panel layout in panel.toml, so the desktop settings file stays readable.
      GIVEN [[panels]] only in ~/.config/glimpse/panel.toml WHEN the panel starts THEN it uses it.
      Verify by: load test over a temp root with panel.toml.
AC-2  As a user, I want a typo in panel.toml to leave my daemons alone.
      GIVEN invalid panel.toml WHEN glimpse-idle loads THEN it loads config.toml without error.
      Verify by: load(Documents::Config) test with a broken panel.toml beside it.
AC-3  As a user, I want misplaced tables named, so I know which file to edit.
      GIVEN [lock] in panel.d/10.toml WHEN loading ConfigAndPanel THEN the error names config.toml.
      GIVEN [[panels]] in config.toml WHEN loading either THEN the error names panel.toml.
      Verify by: two load tests.
AC-4  As a developer, I want --config-dir stacked on top, so a test overlays only what it sets.
      GIVEN --config-dir D with config.toml setting one key WHEN loading THEN user and /etc layers
      merge beneath it. Verify by: config_files order test over explicit roots.
AC-5  As a developer, I want writes to land in the top root.
      GIVEN --config-dir D WHEN the wallpaper portal accepts an image THEN D/config.toml changes and
      ~/.config/glimpse does not. Verify by: writable_file test plus a live run with temp XDG_CONFIG_HOME.
AC-6  As a user, I want one watcher per process.
      GIVEN a running panel WHEN listing its threads THEN exactly one "notify-rs inotify loop" serves
      config and theme (service watchers excepted). Verify by: live check of /proc/<pid>/task/*/comm.
AC-7  As a user, I want an unreadable stylesheet to keep the last good styling.
      GIVEN styles.css replaced by a directory WHEN the watch fires THEN watch_theme yields Rejected
      and the panel keeps its CSS. Verify by: watch_theme test.
AC-8  As a user, I want a theme from /etc/glimpse/themes found.
      Verify by: find() test over explicit roots.
AC-9  As a new user, I want stub files that point at the schema and docs.
      GIVEN no config files WHEN the panel starts THEN config.toml and panel.toml hold the stubs.
      GIVEN --config-dir WHEN starting THEN nothing is seeded. Verify by: seed tests.
AC-10 As a maintainer, I want no compatibility code left.
      Verify by: inspection — no config.d, shot.toml, shadowing_dropin, retired_*, GLIMPSE_CONFIG_PATH.
```

## Doc edits this change forces

- `crates/glimpse-config/README.md` "Layers", "Generated references" and "Writing": rewritten to
  the root stack; replaces the existing paragraphs, shorter.
- `AGENTS.md` testing bullet "Never test against ~/.config/glimpse: pass --config" becomes "pass
  `--config-dir` and a temporary `XDG_CONFIG_HOME`"; same bullet, no new one.
- `docs/live-testing.md`, `docs/handbook.md`, `.claude/skills/testing`, `.claude/skills/live-testing`,
  the READMEs of `glimpse-settings-daemon`, `glimpse-shot`, `glimpse-utils`, `glimpse-weather`, and
  the scripts using `--config`: replace the flag.
- `website/config/`: the file layout page; `website/start/upgrade.md`; every website page that puts
  `[[panels]]`/`[applets]` in `config.toml`; `website/guide/theming.md` (roots, relative `url()`).
- Packaging, CI, install script, website schema loader and the translation template: glimpse-3opwd.14.

## Struck

- Per-binary files (`idle.toml`, `lock.toml`, …): every shared table forces cross-file reads.
- A shared `glimpse.toml` globals file beside per-binary files: same cost, more files.
- A separate `Panel` struct or `Document` trait: changes the service bound and `glimpsectl`.
- Moving schema types into binary crates: services and `glimpsectl` read them, and nothing depends
  on a binary crate.
- Watchers in `glimpse-utils`: drags notify, tokio and toml into crates that read no config.
- A reference-counted watcher hub: a fixed watch over the writable roots covers config and theme.
- `DATA_DIR/config.toml` layer: nothing ships there.
- `--config-dir` replacing the user layer: isolation belongs to `XDG_CONFIG_HOME`.

## Open questions

- ~~Shared appearance tables~~ — stay in `config.toml`.
- ~~`shot.toml`~~ — folded into `config.toml`.
- ~~Service tables~~ — stay in `config.toml`.
- ~~Override~~ — `--config-dir`, stacked on top.
- ~~Theme reload type~~ — `Reload<Theme>` with contents.
- ~~Migration~~ — none.
