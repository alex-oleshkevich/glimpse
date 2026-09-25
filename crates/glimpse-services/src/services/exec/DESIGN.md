# External applets — implementation plan

Status: **approved 2026-09-24; filed as beads (see §12a).**

- **Where the work happens:** the worktree `.claude/worktrees/external-applets`, on branch
  `worktree-external-applets`.
- **Before any slicing:** this plan is copied into the tree as
  `crates/glimpse-services/src/services/exec/DESIGN.md`, because `var/` is gitignored.
- **Evidence:** `var/external-applets/{design.md,exploration.md,proto/}`.
- **Tags:** **[measured]** means run on this machine; **[cited]** means read in a spec or in
  source.

## Context

Any program can put chips on the glimpse bar and fill a popover, the way COSMIC applets do.

- **Declaring an applet:** it ships a `.desktop` file declaring
  `Implements=me.aresa.Glimpse.Applet1`.
- **Placing it:** the user writes its desktop id in a panel zone.
- **Running it:** the panel runs its `Exec` and speaks NDJSON over stdin and stdout.
- **Deno applets** use `Exec=glimpse-applet <entry>`, which picks the Deno flags. The SDK
  `@glimpse/applet` (React) is the easy way to write one.
- **Drawing:** the panel draws only its own widgets.

**User decisions, 2026-09-24:**
- the glimpse widgets plus a small GTK layer;
- an exec service inside the panel;
- options as an `options` subtable, passed through;
- host services (notify, clipboard, open URI, session) with a click gate only;
- the SDK and a template;
- discovery from `.desktop` files, with `Exec` run as is;
- placement by desktop id in a zone;
- a `glimpse-applet` launcher that exec's an **external** deno (the distro package or `~/.deno/bin`), which is neither shipped nor embedded;
- `glimpsectl applets new` (an interview whose every question a flag overrides), `dev`, `check`, `bundle`, `install`/`uninstall`, `list`, `inspect`, `restart`;
- **no `applet.toml`**: the `.desktop` file is the whole manifest;
- **one process per bar placement**, and the applet receives that placement (output, position,
  orientation, zone, size) at init and whenever it changes;
- **keep it simple.**

## 1. Current state

- **Config:** `AppletKind::Exec {}` is a fieldless stub (`crates/glimpse-config/src/schema/applets.rs:77-78`).
- **Panel:** the build match maps it to `None` (`crates/glimpse-panel/src/applets/mod.rs:322`), and
  the panel logs "applet is not implemented yet, skipping" (`components/panel.rs:310-314`).
- **Process supervision:** nothing in the tree supervises a long-lived child. The only child
  processes are one-shot `.output()` calls in `services/ruler/runner.rs:64-83` and
  `color_picker/picker.rs:43-64` [cited].
- **What this plan adds:**
  - config: the `Exec` struct and a desktop-id fallback;
  - in glimpse-services: an `exec` service and `DesktopCatalog`;
  - in glimpse-panel: an `exec` applet;
  - a new `glimpse-applet` crate;
  - `sdk/applet/`;
  - `glimpsectl applets new|list|inspect`.

## 2. Local code this builds on

| Need | Where |
| --- | --- |
| Zone name → applet: the one fallback every caller uses | `Applet::from_name` `applets.rs:794-798`; callers `resolve_applet` `:907-912`, `named_applets_exist` `load.rs:202-223`, `placed_kinds` `schema/mod.rs:146-152` |
| Table → applet | `entry()` `applets.rs:815-831`, which inserts `extends = name` when absent (`:817-819`) |
| Config type re-exports | `schema/mod.rs:34-45`: `Command as CommandAppletConfig`, … |
| Instances of one kind | the test at `applets/mod.rs:350-361` |
| Applet trait, `Input`, `Ctx::watch`, `Opener` | `applet/mod.rs:18-46,49-86,178-199,261`; catcher typing `catcher.rs:216-224` |
| Panic isolation per applet | `catch_unwind` at `runtime.rs:164,210,264,299-304`. **Signal closures are not covered.** |
| Service trait, `initial_state`, `Sub::stream`, `Sub::deadline`, `Sub::interval` | `service.rs:71-100,84`; `subscription.rs:30-65` |
| Keyed children | the tray, `tray/mod.rs:32-46,239-261` |
| A restart counter in a SubKey | geolocation `attempt: u64` at `geolocation.rs:88,123` |
| Service panic isolation | `service.rs:351-372` |
| Stream backpressure | `context.rs:163` |
| Injected, fakeable dependencies | `services.rs:141-168`: `ProcessPicker`, `Arc<dyn Selection>` |
| Offering text to the clipboard | `Selection::offer(Offer{mime,data})` (`selection.rs:11-19,66-70`); pattern at `color_picker/mod.rs:201-208` |
| Posting a notification | `NotificationsProviderHandle::post(app_name, app_id, icon, summary, body, urgency)` (`glimpse-dbus/src/clients/notifications.rs:405-420`); tests use `NotificationsProvider::unavailable(r).handle()` (`:294,318`) |
| Session actions and their confirmation | `AppInput::SessionConfirm` / `SessionRun` sent as `session/mod.rs:100-121` does; `session::render::confirm` (`session/render.rs:69-100`), whose module is private (`session/mod.rs:1`) and becomes `pub(crate)` |
| Opening a URI | `launch_default_for_uri_future` (`places/indicator.rs:99`), given `gdk::Display::app_launch_context()` so the token flows (`popover.rs:39-48`) |
| Keyed reconcile | `glimpse_widgets::reconcile::by_key` (`reconcile.rs:3`); both the module (`lib.rs`) and the fn become `pub` |
| Desktop lookup already inside services | `notifications.rs:7,646-667` (`DesktopAppInfo`) |
| systemd user manager proxy | `glimpse-dbus/src/clients/systemd1.rs:16-48` |
| glimpsectl loading, rendering, `--json`, timeouts | `commands/config.rs`, `render.rs`, `commands/mod.rs:34,49`, `cli.rs:170-174` |

## 3. Facts

| Fact | Tag |
| --- | --- |
| Session panel: 38 MB PSS. A Deno applet: ~43 MB PSS, +15 MB for each further one | measured |
| COSMIC ships 13 applets as `NoDisplay` `.desktop` files with `X-CosmicApplet=true` and a plain `Exec` | measured |
| React 19.3 + react-reconciler 0.34 run on Deno 2.9.7; a bundle needs no permission flags | measured |
| `deno --watch` restarts in place: same PID, and the new module speaks first | measured |
| SDK crash modes: a render throw leaves the process alive (the SDK exits 70); a handler throw or unhandled rejection exits 1; the heap cap exits 133; a busy loop cannot be detected | measured |
| PDEATHSIG: no orphan after `kill -9` of the host. It is **per spawning thread** | measured / cited (prctl(2)) |
| Without `seq`, a controlled entry loses keystrokes | measured |
| Deno is not on PATH here; its installer puts it in `~/.deno/bin`, which the systemd user PATH lacks | measured / cited |
| systemd-oomd inactive; panel unit `MemoryMax=infinity`, `KillMode=control-group` | measured |
| A zombie keeps its PID until reaped, so adopting it into a scope before `wait()` cannot hit a reused PID | cited |

## 3a. Freedesktop integration

The `.desktop` file an applet ships is its whole manifest:

```ini
[Desktop Entry]
Type=Application
Version=1.5
Name=Pomodoro
Comment=A focus timer
Icon=me.example.Pomodoro
Exec=glimpse-applet /usr/share/me.example.Pomodoro/applet.js
TryExec=glimpse-applet
NoDisplay=true
Implements=me.aresa.Glimpse.Applet1
```

| Rule | Why | Tag |
| --- | --- | --- |
| Marker: `Implements=me.aresa.Glimpse.Applet1` | spec §9 exists for this; the name is versioned, and GIO indexes it | cited (gdesktopappinfo.c L1248) |
| Discovery: `DesktopAppInfo::implementations(INTERFACE)` | GIO applies precedence (`XDG_DATA_HOME` first), maps subdirectory ids to dashes, and drops `Hidden` and a failed `TryExec` | cited (L4791) |
| Resolving one id: `DesktopAppInfo::new("<id>.desktop")`, then refuse when `is_hidden()`, when `string_list("Implements")` (gio-unix `v2_60`) lacks the interface, when `DBusActivatable`, or when `Terminal` | `new` does not skip `Hidden`, but it does return `None` for a missing file, a non-`Application` type, a failing `TryExec` and an `Exec` program not on `PATH` — so a `None` for an existing file says so rather than "no entry"; stdio cannot cross D-Bus activation or a terminal | cited (L2186, L2206) |
| Keep `NoDisplay`; never `OnlyShowIn=Glimpse` | not a desktop (`XDG_CURRENT_DESKTOP=niri`); the validator treats it as fatal | measured / cited |
| Exec expansion: `gio::glib::shell_parse_argv`, then drop an argument that is exactly `%f %F %u %U %d %D %n %N %v %m`, turn a standalone `%i` into `--icon <Icon>` (or nothing); inside any remaining argument `%%` → `%`, `%c` → the Name, `%k` → the path, `%f %u %d %n %v %m` → removed, and any other code (including `%F %U %i` embedded, or a trailing `%`) → refuse. An empty `Icon=`/`Path=` is absent. `Path=` is the cwd | GIO's expansion is private, and its launcher returns only a pid and double-forks. About 20 lines, marked `ponytail:` | cited (L2689, L3673) |
| Own scope: `app-glimpse-<id with - as \x2d>-<pid>.scope` in `app.slice`, made by `StartTransientUnit(name,"fail",[PIDs,Slice,CollectMode=inactive-or-failed],[])` **before the first `wait()`** | portal/notification identity, cgroup separation from the panel, and it is how `inspect` finds the process. A failure is logged once and ignored | cited + measured |

## 4. Architecture

```
applet (any language)                glimpse-panel
┌──────────────────────┐ stdout    ┌──────────────────────────────────────────────┐
│ Exec=glimpse-applet  │ ────────▶ │ exec service (glimpse-services)              │
│  → execvp deno …     │  NDJSON   │  source per child: resolve (spawn_blocking),  │
│  (or any binary)     │ ◀──────── │   spawn + adopt (async task), reader, writer, │
└──────────────────────┘  stdin    │   child stderr discarded                      │
                                   │  handler: status, generations, opens, gate,   │
                                   │   notify, copy, requests → watch<ExecState>   │
                                   ├──────────────────────────────────────────────┤
                                   │ exec applet (one per zone entry per bar)      │
                                   │  owns one slot = one child; sends placement;  │
                                   │  chips, popover, carries out requests         │
                                   └──────────────────────────────────────────────┘
```

**How a failure is contained:**
- **The child** is another process and its own cgroup, so its crash, OOM or hang stays there.
- **Decoding:** every decode is a `Result`, and a violation stops that one child.
- **Applet code:** `handle`, `indicators` and `popover` are covered by the runtime's
  `catch_unwind`.
- **Signal closures are not covered, so they must be panic-free.** Numbers go through
  `serde_json::Number::from_f64` as an `Option`, with no `unwrap`.
- **A service panic** stops the exec service only (`service.rs:351-372`), never the panel.

## 5. Types

### Config — `applets.rs`, re-exported as `ExecAppletConfig`

```rust
/// A third-party applet found through its `.desktop` file (`Implements=me.aresa.Glimpse.Applet1`).
Exec(Box<Exec>),        // replaces `Exec {}` at :78

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Exec {
    /// The desktop-file id, e.g. `me.example.Pomodoro`.
    pub applet: String,
    /// Passed to the applet as written.
    #[serde(default)]
    pub options: serde_json::Map<String, serde_json::Value>,
}

pub fn is_desktop_id(name: &str) -> bool;  // ≥2 dot-separated segments of [A-Za-z_][A-Za-z0-9_-]*
pub fn placed_applets(config: &Config) -> impl Iterator<Item = (&str, Applet)> + '_; // beside placed_kinds
```

- **`from_name(name)`:** when `Kind::deserialize` fails and `is_desktop_id(name)` holds, it returns
  `Exec{applet: name, options: {}}`. A bare `exec` (missing `applet`) and a typo with no dot stay
  "unknown applet".
- **`entry(name, table)`:**
  - A desktop-id table name with no `extends` gets `extends="exec"` and `applet=name` inserted.
  - An `applet` value that fails `is_desktop_id` is rejected, naming the key.
- **Documentation:** the commented-config exclusion of `exec` stays (`commented.rs:177`). The
  crate README documents the table instead. `just gen-config-schema` is regenerated.
- **`tooltip-format`** is accepted and ignored for exec; `settings-label`/`settings-command` work.

### Catalog — `glimpse-services/src/services/exec/catalog.rs`

```rust
pub const INTERFACE: &str = "me.aresa.Glimpse.Applet1";

pub trait Catalog: Send + Sync + 'static {
    fn resolve(&self, id: &str) -> Result<Entry, String>;     // blocking
}
#[derive(Debug, Clone, PartialEq)]
pub struct Entry { pub argv: Vec<String>, pub cwd: Option<PathBuf>, pub name: String,
                   pub icon: Option<String>, pub path: PathBuf, pub exec: String }
pub struct DesktopCatalog;                                   // impl Catalog, the §3a rules
impl DesktopCatalog { pub fn installed(&self) -> Vec<(String, Result<Entry, String>)>; }
pub fn expand(exec: &str, name: &str, icon: Option<&str>, path: &Path) -> Result<Vec<String>, String>;
```

glimpse-services inherits the workspace `gio` (`Cargo.toml:32`) for `gio::glib::shell_parse_argv`.

### Wire — `exec/wire.rs`

The applet speaks first. Framing is `LinesCodec::new_with_max_length(1 << 20)`.

```rust
#[derive(Deserialize)] #[serde(tag = "t", rename_all = "kebab-case")]
pub enum FromApplet {
    Hello { v: u32 },
    Commit { ops: Vec<Op> },
    Notify { summary: String, #[serde(default)] body: String,
             #[serde(default)] icon: Option<String>, #[serde(default)] urgency: Urgency },
    Copy { text: String },
    OpenUri { uri: String },
    Session { action: SessionVerb },
    ClosePopover,
}

#[derive(Deserialize)] #[serde(tag = "op", rename_all = "lowercase")]
pub enum Op {
    Insert { parent: u32, node: WireNode, before: Option<u32> },
    Move { parent: u32, id: u32, before: Option<u32> },
    Remove { parent: u32, id: u32 },
    Set { id: u32, props: serde_json::Map<String, Value>, #[serde(default)] seq: Option<u64> },
}
#[derive(Deserialize)]
pub struct WireNode { pub id: u32, #[serde(rename = "type")] pub kind: String,
                      #[serde(default)] pub props: serde_json::Map<String, Value>,
                      #[serde(default)] pub children: Vec<WireNode> }
#[derive(Deserialize, Default, Clone, Copy, Debug, PartialEq)] #[serde(rename_all = "lowercase")]
pub enum Urgency { Low, #[default] Normal, Critical }
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)] #[serde(rename_all = "kebab-case")]
pub enum SessionVerb { Lock, Suspend, Hibernate, LogOut, Reboot, PowerOff }

#[derive(Serialize, Debug)] #[serde(tag = "t", rename_all = "lowercase")]
pub enum Outgoing {                                  // owned: it crosses a channel
    Hello { v: u32, name: String, options: serde_json::Map<String, Value>, placement: Placement },
    Options { options: serde_json::Map<String, Value> },
    Placement { placement: Placement },
    Event { id: u32, name: String, args: Vec<Value>, seq: Option<u64> },
    Popover { open: bool },
}

#[derive(Serialize, Debug, Clone, PartialEq)] #[serde(rename_all = "kebab-case")]
pub struct Placement {
    pub output: Option<String>,          // connector, e.g. "DP-2"; ctx.output()
    pub position: Edge,                  // the panel's configured `position`
    pub orientation: Orientation,        // what Applet::orient delivers
    pub zone: Zone,                      // left | center | right
    pub size: u32,                       // the panel's configured `size`
}
#[derive(Serialize, Debug, Clone, Copy, PartialEq)] #[serde(rename_all = "lowercase")]
pub enum Edge { Top, Bottom, Left, Right }
#[derive(Serialize, Debug, Clone, Copy, PartialEq)] #[serde(rename_all = "lowercase")]
pub enum Orientation { Horizontal, Vertical }
#[derive(Serialize, Debug, Clone, Copy, PartialEq)] #[serde(rename_all = "lowercase")]
pub enum Zone { Left, Center, Right }
```

`Edge` mirrors `glimpse_config`'s panel `Position` (`schema/panels.rs:10`), and the builder maps it
at the call site. It is not re-used directly, because a wire type must not change when a config
type does.

### Tree — `exec/tree.rs`

It is Appendix A: `Tree`, `Node`, a service-side `Severity`, `Element` (the glimpse set,
the §12a GTK layer, `Unsupported`), per-key merge where `null` deletes, text caps, the icon regex,
and non-finite numbers dropped. `IndicatorProps` is extended:

```rust
pub struct IndicatorProps {
    pub icon: Option<String>, pub text: Option<String>, pub tooltip: Option<String>,
    pub badge: Option<String>, pub overlay: Option<String>, pub dot: Option<String>,
    pub severity: Option<Severity>, pub attention: bool, pub notice: bool,
    pub class_name: Option<ClassName>, pub on_press: bool, pub on_scroll: bool,
}
pub const MAX_NODES: usize = 300;
pub const MAX_UPDATES_PER_SECOND: u32 = 120;
pub const MAX_CHILDREN_PER_PARENT: usize = 200;
// Tree { nodes: HashMap<u32, Arc<Node>> } — Arc per node so an untouched node stays ptr_eq (§6a)
// plus the §6a parent table, enforced at commit validation
```

### Service — `exec/mod.rs`

Each process belongs to one **slot**, meaning one exec applet instance on one bar. Bars are
created at runtime (outputs come and go), so a bar **attaches** its slot rather than the document
listing slots. The document supplies each instance's `applet` id and `options`.

```rust
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExecState { pub slots: BTreeMap<u64, SlotState> }

#[derive(Debug, Clone, PartialEq)]
pub struct SlotState {
    pub applet: String,                              // instance name (zone entry / table name)
    pub generation: u64,
    pub tree: Arc<Tree>,
    pub status: Status,
    pub title: String, pub icon: Option<String>,     // from Entry, for notifications
    pub requests: Vec<(u64, BarRequest)>,            // last 8, serial ascending
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status { Starting, Running, Restarting { attempt: u32 }, Failed(String) }
#[derive(Debug, Clone, PartialEq)]
pub enum BarRequest { OpenUri(String), Session(SessionVerb), ClosePopover }

#[derive(Debug, Clone, PartialEq)]
pub struct Config { pub applets: BTreeMap<String, glimpse_config::ExecAppletConfig> }
// From<&glimpse_config::Config>: placed_applets() filtered to Kind::Exec

pub struct Dependencies { pub catalog: Arc<dyn Catalog>, pub selection: Arc<dyn Selection>,
                          pub notifications: NotificationsProviderHandle }

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SubKey { Child { slot: u64, spawn: u64 }, Backoff { slot: u64, spawn: u64 }, Retry }

pub enum Event {
    Spawned { slot: u64, spawn: u64, link: Link, entry: Entry },
    Hello { slot: u64, spawn: u64 },
    Tree { slot: u64, spawn: u64, tree: Arc<Tree> },
    Request { slot: u64, spawn: u64, request: FromApplet },  // Notify|Copy|OpenUri|Session|ClosePopover only
    Exited { slot: u64, spawn: u64, spoke: bool, ran: Duration, reason: String },
    Backoff { slot: u64 },
    Retry,
}
pub struct Link { pub tx: mpsc::Sender<Outgoing>, pub stop: CancellationToken }  // queue 64

pub struct UserEvent { pub slot: u64, pub generation: u64, pub id: u32, pub name: String,
                       pub args: Vec<Value>, pub seq: Option<u64>, pub gesture: bool }

impl ExecHandle {
    pub fn snapshot(&self) -> ExecState;
    pub fn subscribe(&self) -> watch::Receiver<ExecState>;
    pub async fn attach(&self, slot: u64, applet: &str, placement: Placement) -> Result<(), CommandError>; // exec.attach
    pub async fn place(&self, slot: u64, placement: Placement) -> Result<(), CommandError>;               // exec.place
    pub async fn detach(&self, slot: u64) -> Result<(), CommandError>;                                     // exec.detach
    pub async fn send_event(&self, event: UserEvent) -> Result<(), CommandError>;                          // exec.send_event
    pub async fn popover(&self, slot: u64, open: bool) -> Result<(), CommandError>;                        // exec.popover
}
```

- **The slot id** is `static NEXT: AtomicU64` in the panel, taken in `Exec::start`, so it is unique
  for the process lifetime and never reused.
- **Detach:** `impl Drop for exec::Exec` sends `exec.detach`. The runtime drops an applet on
  rebuild, on output removal and in its panic `stop()` (`runtime.rs:270-284`), so every path
  detaches.
- **Seeding:** `initial_state` is empty. Until `attach` lands, `slots[slot]` is absent, which means
  no chips.
- **An attach naming an instance the config lacks** becomes `Failed("not configured")`.
- **Config changes:**
  - an `options` change sends `Options` to every slot of that instance, keeping the pid;
  - an `applet` change respawns them;
  - a removed instance fails its slots until the panel drops them.
- **Counters:** `spawn` and `generation` are service-global and only increase, so a `--watch`
  reload (same pid, new `Hello`) gets a fresh generation and old events are dropped. `requests`
  serials are per slot and only increase.

### Spawn — `exec/spawn.rs`

1. `resolve` runs in `spawn_blocking`.
2. **`Command::spawn` runs back on the async source task, never in the blocking closure.**
   PDEATHSIG fires when the *spawning thread* exits, and pool threads retire after about 10 s.
3. The command gets `argv`, `cwd`, piped stdin/stdout, discarded stderr and `kill_on_drop(true)`, plus
   `pre_exec(set_parent_process_death_signal(Some(Signal::KILL)))`. That adds the `process`
   feature to the workspace `rustix` (`Cargo.toml:62`).
4. The environment is inherited.
5. `scope::adopt` runs **before any `wait()`**.
6. **Removal:** `start_kill()`, then `wait()` in a detached task.

### Launcher — `crates/glimpse-applet` (new binary, a leaf)

```
glimpse-applet [--allow-net=HOSTS] [--allow-read=PATHS] [--allow-env=VARS] [--watch] <entry.{js,ts,tsx}>
→ execvp(deno, run -q --no-prompt --v8-flags=--max-old-space-size=128 [--watch]
               --allow-env=NODE_ENV[,VARS] [--allow-net=…] [--allow-read=…] <entry>)
```

- **Finding Deno:** `$GLIMPSE_DENO`, then `PATH`, then `$HOME/.deno/bin/deno`, the installer
  default that the systemd PATH lacks.
- **When it is missing:** exit with "deno not found; install it or set GLIMPSE_DENO". The panel
  reports that the child exited before `Hello`.
- **What it cannot grant:** `-A`, `--allow-run`, `--allow-ffi`, `--allow-write` and
  `--allow-sys` cannot be expressed.
- **Why `exec`:** it keeps the pid, the pipes, the scope and PDEATHSIG.
- **Tests:** the argv builder is a pure fn, tested.
- **Code shape:** `run(cli) -> anyhow::Result<()>` plus `errors.rs`, per AGENTS.md. The crate is
  added to the justfile `binaries` (`justfile:14`) and to the three package lists.

## 6. Flows

**Place.**
1. `center = ["me.example.Pomodoro"]` passes `named_applets_exist` through `from_name`.
2. `exec::Config::from` → `placed_applets` gives the instance's id and options.
3. Each bar that builds the entry runs the `Exec` arm of `applets::build`. The arm receives a
   `Placement` built by `components/panel.rs` at its `build` call site (`:282`):
   - `output` from the bar's output;
   - `position` and `size` from the `Panel` config (`schema/panels.rs:7-10`);
   - `zone` from the zone being filled;
   - `orientation` from the panel orientation.
4. `Exec::start` takes a slot id and sends `exec.attach(slot, name, placement)`.
5. `Applet::orient` (`applet/mod.rs:30`) sends `exec.place` only when the orientation actually
   changes. So does a `configure` that changes `position` or `size`.

**Start.** One `Child{slot, spawn}` sub per attached slot.
1. The source resolves the entry and spawns the process.
2. It adopts the process into its scope.
3. It emits `Spawned{link, entry}`; the handler stores the link and sets `title`/`icon`.
4. On a resolve error → `Exited{spoke:false, reason}` → `Failed(reason)`.

**Hello.**
1. The child writes `Hello{v}`, and the source emits `Event::Hello`.
2. The handler increases `generation`, resets the tree, and sets `Running`.
3. It replies with `Outgoing::Hello{v:1, name, options, placement}`.
4. If this slot's popover is open, it also sends `Popover{open:true}`.
5. The SDK mounts on that reply. The same thing happens on every `--watch` reload.
6. A later `exec.place` with a different placement sends `Outgoing::Placement`, keeping the pid.

**Commit.**
1. The batch is validated against a copy of the tree: ids, cycles, parent kind, `MAX_NODES`, and
   the commit rate.
2. If valid → `Tree`. Any violation → the child is cancelled with the reason.
3. `send().await` provides backpressure.

**Bar (on `Woken`).**
1. **Carry out `requests` first:** those with a serial above the last one seen.
2. Then return if `ptr_eq(tree)` shows nothing changed. A missing `slots[slot]` means no chips.
3. `indicators()` maps the root's `Indicator` children.

**Pointer.** A press other than left, or a scroll, goes to the first indicator that has
`onPress`/`onScroll`, with `gesture: true`. A left press opens the popover.

**Open.**
1. Render only the applet's hero, body, and footer nodes; hide empty areas.
2. `map`/`unmap` → `exec.popover(slot, open)`, written straight to that slot's child.
3. Widgets are keyed by `(generation, id)`.

**Interact.**
- **Which signals are gestures:** `clicked` (Row, Button), `activate` (Entry submit) and pointer
  events send `gesture: true`. `changed`/`toggled`/`value-changed` send `gesture: false`.
- **Dressing is quiet:** every GTK-layer handler is blocked while the panel sets the widget's
  value, the way `SwitchRow`/`Fader` already guard themselves. The panel's own writes never echo
  back as events.
- **The service:**
  - drops an event from a stale generation;
  - when `gesture` is true, sets the slot's `last_gesture: Instant`;
  - queues the event with `link.tx.try_send`. `Full` cancels the child with "not reading stdin",
    so the handler never awaits a child.

**Type.** `seq`: each user change bumps `local_seq[(generation, id)]`; the SDK stamps that seq on the next `set` carrying the value; when dressing, a node whose seq is below the local one keeps the widget value; a `set` with no seq applies.

**Host requests.**
- **`Notify`** is not gated, because timers need it.
  - It is posted as `(title, applet id, icon or entry icon, summary, body, urgency)` through a
    pure `note(entry, id, request)`.
  - Anything within 1 s of the previous post is dropped and logged.
- **The other four are accepted only within 2 s of `last_gesture`.** Otherwise they are dropped,
  with one warning.
  - `Copy` → `selection.offer(Offer{TEXT, bytes ≤ 1 MiB})`.
  - `OpenUri`, `Session` and `ClosePopover` → the slot's `requests`.
- **The slot's bar carries them out:**
  - `OpenUri` → `launch_default_for_uri_future(uri, Some(&display.app_launch_context()))`.
  - `Session` → `session::render::confirm`, then `SessionConfirm` or `SessionRun`. The exec arm
    receives the `SessionActionsHandle` and `dialog`.
  - `ClosePopover` → `opener.close_popover()`.

**Death.**
1. `Exited` → `attempt = if ran >= 60 s { 0 } else { attempt + 1 }`.
2. If the child spoke → `Restarting{attempt}` plus a `Sub::deadline(Backoff{slot, spawn})` at
   `min(2^attempt, 60)` s → `Backoff` → a new `spawn` and `Starting`.
3. If it never spoke → `Failed(reason)`.

**Retry.** While any slot is `Failed`, one `Sub::interval(Retry, 5 s)` re-resolves the failed
slots. It respawns those whose entry now resolves (installing after placing just works) or whose
config changed. `Failed` is logged once per reason.

**Detach.** `exec.detach(slot)` drops the slot's key. The child gets `start_kill()`, then a
detached `wait()`.

## 6a. GTK reconciliation (panel side, `crates/glimpse-panel/src/applets/exec/dom.rs`)

The service publishes a data tree. The bar turns it into widgets, **incrementally**, so that
focus, the caret, a drag and scroll state all survive every commit.

### Shape

```rust
type Key = (u64 /*generation*/, u32 /*id*/, ElementKind /*discriminant*/);

pub struct Dom {
    shell: glib::WeakRef<PopoverShell>,
    body: gtk4::Box,                     // the one child handed to PopoverShell::set_content
    footer: gtk4::Box,                   // appended to the footer once
    hero: Option<(Key, Hero)>,
    nodes: HashMap<Key, Mounted>,
    held: HashMap<Key, Vec<(Key, gtk4::Widget)>>,   // per container: what by_key holds
}
struct Mounted {
    widget: gtk4::Widget,
    inner: Option<gtk4::Box>,            // a container's child box (Section content, Box itself)
    node: Arc<Node>,                     // last dressed; ptr_eq → skip
    handlers: Vec<glib::SignalHandlerId>,
}
```

`Tree` stores `Arc<Node>`, so a commit that touches one node replaces one `Arc`, and every
untouched node stays `ptr_eq`. That changes `design.md`'s `HashMap<u32, Node>` to
`HashMap<u32, Arc<Node>>`, which also makes the reader's validate-against-a-copy cheap.

### Algorithm, on each `Woken` while the popover is shown

1. `shell.upgrade()`. If it returns `None`, the popover was dismissed: drop the `Dom` and stop.
2. **Hero.** The `Popover` node's `Hero` child: when it appears, `set_hero`; when it leaves,
   `clear_hero`. A different key rebuilds it; otherwise it is dressed.
3. **Containers.** For each container node (the Popover body, each `Section`, each `Box`, the
   `Footer`), `glimpse_widgets::reconcile::by_key(inner, held[key], children, key, build, apply)`
   runs with `W = gtk4::Widget`:
   - `build` creates the widget for the element kind and connects its handlers **once**.
   - `apply` returns immediately when `ptr_eq(mounted.node, node)`. Otherwise it dresses.
   - It recurses into child containers **before** placing them, so a subtree is complete before
     it is inserted.
4. **Dressing** is compare-before-write per property.
   - For the GTK layer, every write happens between `block_signal(h)` and `unblock_signal(h)` for
     that widget's handlers.
   - `SwitchRow` and `Fader` already guard themselves (`quiet`, `fader/imp.rs:20,181`,
     `switch_row/imp.rs:21,33`).
5. **Visibility sync** (pitfall 1): `body`, `footer` and each Section's inner box are each set to
   `visible = has a child`.
6. **Typing:** `opener.typing(the tree has an Entry or a Scale)`.
7. **Stale keys:** a key missing from the new tree has its `Mounted` dropped. That drops its
   handlers with its widget; `by_key` has already unparented it.

**On open,** `popover()` builds the shell and the `Dom` in one pass, from the current tree, with
no round trip. `map`/`unmap` send `exec.popover`, and `unmap` also calls `opener.wake()`, so the
next `Woken` finds `upgrade() == None` and drops the `Dom`.

### Which parent may hold which child

The service enforces this table at commit validation, so the panel never sees anything else. A
violation is a protocol error.

| Parent | May contain |
| --- | --- |
| root | `Indicator`\*, at most one `Popover` |
| `Popover` | at most one `Hero`, at most one `Footer`, and any body element |
| `Section` | body elements except `Section` |
| `Box` | body elements except `Section` |
| `Footer` | `Button`, `Row`, `Label`, `Box` |
| every other element | nothing (a leaf; its text is a prop) |

Body elements are `Section`, `Row`, `SwitchRow`, `Fader`, `Entry`, `Placeholder`, `Box`, `Label`,
`Image`, `Button`, `Switch`, `Scale`, `Spinner`, `Progress`, `Separator` and `Unsupported`.

### Pitfalls and their solutions

| # | Pitfall | Where it bites | Solution |
| --- | --- | --- | --- |
| 1 | `PopoverShell::settle` shows the content/footer box only if it had a child **when it was set**, and `watch` listens only to the direct child's `visible` (`popover_shell/mod.rs:128-150`). A popover that opens empty and fills later stays blank | an applet that renders its body after an async fetch | step 5: toggle the inner box's `visible` after each reconcile, which fires `notify::visible` → `settle` |
| 2 | `PopoverShell` has no per-widget footer removal (`append_to_footer`/`clear_footer` only) | a footer button that appears and disappears | append one `footer` Box once; `by_key` its children |
| 3 | `Section::set_content` takes one widget (`section/mod.rs:24`) | rows in a section | the Section's inner `Gtk.Box` is its `Mounted.inner` |
| 4 | `by_key` calls `unparent()` and `insert_after` directly (`reconcile.rs:28-39`), which is only correct for a plain `Gtk.Box` parent; a `ListBox`/`FlowBox` keeps bookkeeping this bypasses | any container | every container's `inner` is a `Gtk.Box`; no `ListBox` anywhere |
| 5 | `by_key` looks keys up linearly (`held.iter().position`, `:18`), O(n²) per container | a Section of 500 rows → 250k compares per commit | fine at popover sizes. `MAX_CHILDREN_PER_PARENT = 200` is enforced at validation, so the worst case is 40k compares |
| 6 | Programmatic writes fire `value-changed`, `notify::active`, `changed`: echo loops, and an applet opening its own click gate | GTK-layer `Switch`, `Scale`, `Entry` | `block_signal` around every write (step 4); gesture only from `clicked`/`activate`/pointer |
| 7 | `set_text` on an `Entry` moves the caret; a stale echo overwrites typing | controlled `Entry` | write only when `text != value` **and** the seq rule allows it; save `position()` before and restore it clamped. Measured in slice 2.3, and recorded in Known state |
| 8 | Removing a focused widget drops keyboard focus, and the catcher keeps `OnDemand` | an Entry removed while typing | step 6 re-derives typing on every reconcile; focus is left to GTK |
| 9 | The same id with a different element (a `--watch` reload reuses ids; a kind change) reuses the wrong widget | reload, kind change | the key is `(generation, id, kind)` |
| 10 | Widgets outliving the popover: a `Dom` holding strong refs keeps a dismissed tree alive, and the next open builds a second one | open, close, open | `shell` is a `WeakRef` (applet skill rule 12); drop the `Dom` when `upgrade()` fails, forced by `unmap` → `wake()` |
| 11 | Signal closures that capture the `Dom` or a widget strongly create cycles | every handler | closures capture only `(ExecHandle, slot, generation, id, name)` and, for values, the widget as `#[weak]`; nothing captures the `Dom` |
| 12 | A panic in a signal closure aborts the panel (not covered by `catch_unwind`) | Fader/Scale numbers | no `unwrap`; `Number::from_f64(v)` → `None` → the event is dropped; NaN/inf test |
| 13 | glib panics on a NaN written to an `f64` property (Known state) | `Fader.value`, `Scale.value` | the tree drops non-finite numbers at validation, and dressing clamps `Scale` to `[min, max]` |
| 14 | Long text widens the popover to the screen, and re-placement makes it jump | `Label`, `Row` title | `Label`: `wrap`, `max-width-chars = 48`, `ellipsize` unless `wrap`. Row and Section text is capped by `clean` (256), and `Row` already ellipsizes |
| 15 | A `Row` is a `Gtk.Button`, so an interactive child inside it nests buttons | a `Button` inside a `Row` | excluded by the parent table (a Row is a leaf) |
| 16 | Building hundreds of widgets in one frame stalls the main loop | the first open of a huge tree | `MAX_NODES = 300` is the ceiling. Nothing is built while the popover is closed |
| 17 | Rebuilding a chip's `gio::Icon` on every `indicators()` call (applet skill rule 3) | a 1 Hz clock applet | a small `HashMap<String, gio::Icon>` cache on the applet, cleared on generation change |
| 18 | The accessible name of an icon-only `Button`/`Image` | screen readers | `tooltip`, when set, also sets the accessible label |

### How it is tested

- **Headless:**
  - the parent table (service validation tests);
  - the seq decision `keeps_newer_local_value`;
  - the prop → widget-value mapping as pure functions in `exec/render.rs`;
  - `Number::from_f64` handling.
- **GTK, in a separate test binary** `crates/glimpse-panel/tests/exec_dom.rs` (`#[ignore]`, run one
  process per test by `just test-crate-compositor glimpse-panel`, per Known state; no seventh
  test in `glimpse-widgets`). Each test drives `Dom` with successive trees and asserts:
  1. an empty popover that later gets a Row shows it (pitfall 1);
  2. a reorder moves widgets and keeps their identity (`==` on the same `Widget`);
  3. a `set` dresses only the touched widget (pointer identity of the others unchanged);
  4. a reload (new generation) replaces every widget;
  5. dressing a `Switch`, `Scale` or `Entry` emits no event (pitfall 6);
  6. `Entry` keeps its caret and text across an older-seq echo (pitfall 7);
  7. removing the shell (drop) → the next reconcile drops the `Dom` (pitfall 10), checked with a
     `WeakRef` on a child widget.

## 7. SDK — `sdk/applet/` (new top level; one AGENTS.md row)

```
sdk/applet/
  mod.ts         run, components, useOptions, usePlacement, usePopoverOpen, useAppletName,
                 notify, copy, openUri, session, closePopover
  reconciler.ts  from var/external-applets/proto/sdk.ts, on the §5 wire; sends Hello first
  mod_test.ts    fixtures/*.tsx → fixtures/*.ndjson; console.log → stderr
  fixtures/      also read by the Rust wire tests (include_str!)
  template/      the scaffold files `glimpsectl applets new` renders (below)
```

- **Components:** `Indicator` (icon, children, tooltip, badge, overlay, dot, severity, attention,
  notice, className, onPress, onScroll), `Popover`, `Hero`, `Section`, `Row`, `SwitchRow`,
  `Fader`, `Entry`, `Placeholder`, `Footer`, and the GTK layer (`Box`, `Label`, `Image`, `Button`,
  `Switch`, `Scale`, `Spinner`, `Progress`, `Separator`).
- **Installed** to `/usr/share/glimpse/sdk/applet/` by `scripts/install.sh` and the three lists in
  `crates/glimpse-package/Cargo.toml` (`:36`, `:72`, `:103`).
- **Tests:** `just sdk-test` runs `deno test`, outside `just verify`.

### `glimpsectl applets` — glimpsectl adds `glimpse-services.workspace = true` and uses `exec::catalog`

- **`new`** interviews the user, then renders the scaffold from template files.

  **The interview.** Every question has a flag, and **a flag overrides its question**: an answer
  given by flag is never asked. `--yes` accepts the default for every remaining question, so
  `glimpsectl applets new --id me.example.Hello --yes` runs with no prompts at all. When stdin is
  not a TTY, a missing answer that has no default is an error naming its flag. That makes CI and
  tests fully flag-driven.

  | # | Question | Flag | Default | Validation |
  | --- | --- | --- | --- | --- |
  | 1 | Applet id | `--id` | — | `is_desktop_id`, and not already installed (`DesktopCatalog::resolve` is `Err`) |
  | 2 | Name | `--name` | the id's last segment, split on case | non-empty |
  | 3 | Description | `--description` | empty | one line |
  | 4 | Icon | `--icon` | `application-x-addon-symbolic` | the §5 icon regex |
  | 5 | Has a popover? | `--popover/--no-popover` | yes | — |
  | 6 | Network hosts (comma-separated) | `--allow-net` | none | no `,` inside a host |
  | 7 | Directory | `--dir` | `./<last segment, kebab-case>` | absent or empty |

  **Template files.** `sdk/applet/template/`, one source of truth, embedded with `include_str!`
  and installed with the SDK:

  ```
  template/
    main.tsx            indicator + popover skeleton (Hero, a Section with one Row)
    main.indicator.tsx  indicator-only skeleton, used when the answer to "popover?" is no
    deno.json           imports @glimpse/applet (DATA_DIR path) + react; tasks: dev, bundle, link, check
    applet.desktop      the §3a shape, rendered as <id>.desktop
    README.md           how to run it, link it, place it, bundle it, ship it
    gitignore           rendered as .gitignore
  ```

  **Rendering.** A pure `render(template: &str, answers: &Answers) -> String` replaces the fixed
  keys `{{id}}`, `{{name}}`, `{{description}}`, `{{icon}}`, `{{exec}}`, `{{dir}}` and `{{sdk}}`
  with `str::replace`. `{{exec}}` is `glimpse-applet --watch [--allow-net=…] <abs dir>/main.tsx`.
  A test asserts that no `{{` survives in any rendered file.
  - A key set this small needs no template engine: std is the first rung, and every value is
    escaped for its file type by the caller. `.desktop` strings are escaped per spec §4, and TS
    strings through `serde_json::to_string`.
  - `new` writes only inside the chosen directory. The printed next steps are
    `glimpsectl applets dev <dir>`, then `center = ["<id>"]`.

  **Proposed new dependency:** `dialoguer = "0.12.0"` (the current release, looked up on crates.io
  2026-09-24), used for `Input` with a validator and for `Confirm`. Nothing in `Cargo.lock` offers
  prompts: none of `dialoguer`, `inquire` or `cliclack` is there. The alternative is a
  `read_line` loop in glimpsectl of about 30 lines. **It lands only on your explicit OK.**
- **`list`** has one row per applet from `DesktopCatalog::installed()`: id, Name, placement
  (instance names, or `—`), `.desktop` path, `Exec`, and state (`running`/`stopped`). Placed ids
  that are not installed come last, as `not installed`. `--json` gives an array.
- **`inspect <id>`** prints three sections:
  - **desktop:** path, Name, Icon, raw `Exec`, and the expanded argv — or the refusal
    reason, from the same `resolve`.
  - **config:** each instance, its zone and panel, and its options as JSON.
  - **runtime** (from systemd, since the panel owns no D-Bus name):
    - `ListUnitsByPatterns(["active","activating"], ["app-glimpse-<esc id>-*.scope"])`;
    - `GetUnitProcesses` for the pids;
    - `MemoryCurrent`;
    - `ActiveEnterTimestamp`.

  When nothing is running, it prints `not running`.
  `--json` gives `{desktop, config, runtime}`.
- **`check [dir]`** (default `.`) runs three checks, stops at the first failure, and exits
  non-zero:
  1. **The `.desktop`:** `<dir>/<id>.desktop` goes through the panel's own rules, via
     `DesktopCatalog::resolve_file(path)`, the same rule function `resolve` uses, fed from
     `DesktopAppInfo::from_filename`.
  2. **`deno check main.tsx`.**
  3. **A handshake:** spawn the expanded argv without `--watch`, wait up to 2 s for `Hello`, reply
     with a sample `Placement`, then wait up to 2 s for a `Commit`. The commit must pass
     `exec::tree`'s validation, **the same code the panel runs**. Then close stdin.
- **`dev [dir]`** is the dev loop against the live panel:
  1. Run `check` steps 1 and 3.
  2. Symlink `<dir>/<id>.desktop`, whose `Exec` is `glimpse-applet --watch …/main.tsx`, into
     `$XDG_DATA_HOME/applications/`. It refuses when a regular file is already there.
  3. If the id is not placed, print the zone line to add.
  4. Wait until Ctrl-C or SIGTERM, then remove the symlink.

  The panel picks the entry up on its next `Retry`, so **`Retry` runs every 5 s** rather than
  30 s. It only runs while some slot is `Failed`, and one `DesktopAppInfo::new` per failed slot is
  cheap. Saving a file reloads through `--watch`.
- **`bundle [dir] [--prefix /usr] [--out dist]`** produces a prefix-shaped tree ready to package:
  1. It runs `check`.
  2. It runs `deno bundle --minify main.tsx -o <out>/share/<id>/applet.js`.
  3. It renders `<out>/share/applications/<id>.desktop` from the dev entry, with
     `Exec=glimpse-applet <same --allow-* flags> <prefix>/share/<id>/applet.js`, no `--watch`,
     and `TryExec=glimpse-applet`. The dev `.desktop` is the single source of the flags.
  4. It copies `<dir>/icon.svg`, if present, to `<out>/share/icons/hicolor/scalable/apps/<id>.svg`.
- **`install [dir]`** is `bundle --prefix ~/.local --out ~/.local`. **`uninstall <id>`** removes
  exactly those three paths. It refuses when the entry's path is not under `~/.local`, because
  that is a distro package.
- **`restart <id>`** calls `KillUnit(<scope>, "all", SIGTERM)` on each running
  `app-glimpse-<id>-*.scope`. The panel sees an exit and respawns after backoff.
- **Deno lookup** (`$GLIMPSE_DENO`, then `PATH`, then `~/.deno/bin/deno`) is one function,
  `glimpse_utils::deno()`, shared by `glimpse-applet` and glimpsectl.
- **Proxy changes:** `Systemd1ManagerProxy` gains `start_transient_unit`,
  `list_units_by_patterns`, `get_unit_processes` and `kill_unit`, and a
  `Systemd1ScopeProxy { memory_current }` is added. Signatures are checked against
  `busctl --user introspect org.freedesktop.systemd1`.
- **No session bus:** none of these is in `needs_session_bus()`. The commands that need a bus
  connect through `within` and print `runtime: unavailable` on failure.
- **Writing to the user's home.** `dev`, `install` and `uninstall` write under `~/.local/share`,
  and only because the user explicitly ran them. AGENTS.md's rule ("a glimpse process writes
  runtime state under `$XDG_RUNTIME_DIR/glimpse/` and nothing else") gains one sentence
  exempting these explicit `glimpsectl applets` commands, in slice 3.x.

## 8. Mockup

`var/widget_examples/exec.blp` is **approved by the user before any panel code.** It shows:
- the applet-supplied hero, body, and footer;
- every glimpse and GTK-layer element;
- an indicator states board (dot, badge, overlay, severity, attention, notice).

It is checked in both schemes.

## 9. Acceptance criteria

```
AC-1  As a user, I want an installed applet on my bar by its id.
      GIVEN ~/.local/share/applications/me.example.Pomodoro.desktop per §3a
      WHEN  center = ["me.example.Pomodoro"] on two bars
      THEN  two children run, one per bar; each bar shows its own child's chips within 2 s
      Verify by: config test desktop_id_in_zone_is_exec; service test with a fake Catalog; live.

AC-1b As an applet author, I want to know where I am drawn.
      GIVEN a bar with position = "left" on DP-2, the applet in the right zone, size 32
      THEN  Hello carries placement {output:"DP-2", position:"left", orientation:"vertical",
            zone:"right", size:32}
      WHEN  the orientation changes at runtime
      THEN  one {t:"placement"} arrives and the pid is unchanged; an unchanged orient() sends
            nothing
      Verify by: unit test Placement from (Panel, zone, output, orientation); service test
                 placement_change_does_not_respawn.
      Verify by: config test desktop_id_in_zone_is_exec; service test with a fake Catalog; live.

AC-2  As a user, I want a typo to stay an error.   GIVEN ["clcok"] or ["exec"]  THEN "unknown applet"
      Verify by: config tests.

AC-3  As a user, I want an applet that cannot run to say why, once, without looping.
      GIVEN no entry; no Implements; Hidden=true in ~/.local; failing TryExec; DBusActivatable;
            Terminal; an unknown field code; a child exiting before Hello
      THEN  Failed(reason) logged once, no chip, no respawn until Retry finds it resolvable
      Verify by: catalog table tests (expand + refusals); service test exits_before_hello_fails.

AC-4  As a user, I want installing after placing to just work.
      GIVEN placed, Failed(not installed)   WHEN the .desktop appears   THEN it runs within 5 s
      Verify by: service test with a fake Catalog flipping to Ok; live.

AC-5  As a user, I want the popover to look like glimpse's own.
      THEN only applet-supplied Hero, body, and Footer in a PopoverShell with glimpse widgets
      Verify by: the approved exec.blp; GTK assertions at the end of widgets().

AC-6  As a user, I want every interactive element to act exactly once.
      GIVEN Row onActivate, SwitchRow onToggle, Fader onChange/onMute, Entry onSubmit, Button,
            Switch, Scale
      THEN  one event each with the right name/args; a dress never emits an event
      Verify by: unit test signal → UserEvent; test that dressing a Switch/Scale/Entry emits nothing.

AC-7  As a user, I want typing never to lose characters.   (design.md AC-4, keyed by (generation, id))
      Verify by: unit test keeps_newer_local_value.

AC-8  As a user, I want a crashed applet to come back without disturbing a healthy one.
      GIVEN Running   WHEN it exits after 10 s   THEN Restarting, respawn after backoff
      GIVEN a restarted child running 5 min   THEN it is never respawned
      Verify by: pure backoff test; service test (one real respawn); key-stability test.

AC-9  As an applet author, I want dev reload to work.
      GIVEN --watch   WHEN the module reloads (same pid, new Hello)
      THEN  the panel replies Hello with options, the tree resets, a queued old-generation
            event is dropped, and an open popover gets Popover{open:true}
      Verify by: service tests hello_is_answered, stale_generation_is_dropped; live.

AC-10 As a user, I want the panel to survive any applet.
      GIVEN garbage; a 2 MiB line; 3000 nodes; a cycle; 500 commits/s; a child that never reads
            stdin
      THEN  that child alone is stopped with a reason; a second applet keeps updating; the
            handler never blocks
      Verify by: service tests rejects_*, full_queue_kills_child, flood_does_not_starve_sibling.

AC-11 As a user, I want no orphan and no early death.
      GIVEN a running child   WHEN the test panel is SIGKILLed   THEN the child is gone in 1 s
      GIVEN a child running 60 s   THEN it is still alive (PDEATHSIG not tied to a pool thread)
      Verify by: live checks.

AC-12 As a user, I want the panel to act on an applet's requests only right after I use it.
      GIVEN no gesture for 3 s   WHEN copy/open-uri/session/close-popover   THEN nothing, one warning
      GIVEN an applet flipping <Switch active> every second   THEN the gate never opens
      GIVEN a click 0.5 s ago   THEN copy offers the text; open-uri/session/close-popover are
            carried out by that slot's bar, even when the applet does not re-render
      Verify by: service tests; bar test that runs requests before the ptr_eq check.
      (A UI guard only: the applet is a user process and could do these things itself.)

AC-13 As a user, I want power actions to ask first.   reboot → the panel's dialog; lock → none
      Verify by: unit test on BarRequest → AppInput; live.

AC-14 As an applet author, I want notifications from a timer.
      THEN one post named by the entry's Name; a second within 1 s is dropped
      Verify by: pure note() test; service test on the drop; live post.

AC-15 As an applet author, I want every chip feature.
      GIVEN dot="#e01b24" badge="3" overlay severity attention notice className
      THEN  each lands in IndicatorSpec; a bad dot or icon is dropped
      Verify by: unit test tree → IndicatorSpec; states board.

AC-16 As a user, I want options to arrive, and to change live.
      THEN  Hello carries them; editing them sends Options, keeping the same pid
      Verify by: service test options_change_does_not_respawn.

AC-17 As a user, I want to list and inspect applets.
      GIVEN two installed (one Hidden-masked), one placed, one placed-but-missing
      THEN  list shows the two with placement/state and "not installed" last; inspect shows
            desktop/config/runtime, the expanded argv identical to the service's, the refusal
            reason for a refused one, and "runtime: unavailable" with no bus; --json matches
      Verify by: pure rows() test; live with XDG_DATA_HOME=<scratch>.

AC-18 As an applet author, I want Deno found and flags fixed.
      THEN glimpse-applet argv has --no-prompt, the heap cap and only the given grants; deno is
           found via GLIMPSE_DENO, PATH or ~/.deno/bin; a missing deno shows "deno not found"
      Verify by: argv/lookup unit tests; live under systemd-run --user.

AC-20 As an applet author, I want a working applet from one interview.
      GIVEN glimpsectl applets new, answered: id me.example.Hello, the defaults, net api.x
      THEN  ./hello holds main.tsx, deno.json, me.example.Hello.desktop, README.md and
            .gitignore; no "{{" remains; Exec=glimpse-applet --watch --allow-net=api.x <abs>/main.tsx;
            the .desktop passes desktop-file-validate; `deno task check` passes
      GIVEN an invalid id, an installed id, or a non-empty dir   THEN re-asked (TTY) or an error
            naming the flag (no TTY)
      GIVEN no TTY and every flag given   THEN the same files, with no prompt
      Verify by: render/escape unit tests; a flags-only integration test in a tempdir;
                 desktop-file-validate + deno check live.

AC-21 As a user, I want to restart a misbehaving applet.
      GIVEN a running applet on two bars
      WHEN  `restart <id>`   THEN each slot's child exits and respawns with a new pid
      Verify by: live restart on two bars.

AC-22 As an applet author, I want dev, check, bundle and install to work end to end.
      GIVEN a dir made by `new --id me.example.Hello --yes`
      WHEN  `check`   THEN desktop rules pass, deno check passes, handshake yields a valid commit
      GIVEN main.tsx that throws on render   THEN check fails at the handshake with its stderr
      WHEN  `dev` with the id placed   THEN chips appear within 5 s; a save reloads; Ctrl-C
            removes the symlink
      WHEN  `bundle --prefix /usr --out dist`   THEN dist/share/<id>/applet.js and
            dist/share/applications/<id>.desktop (Exec=glimpse-applet <flags> /usr/share/<id>/applet.js,
            no --watch) exist and pass desktop-file-validate
      WHEN  `install` then `uninstall <id>`   THEN ~/.local gains, then loses, exactly those paths;
            uninstall of a /usr entry is refused
      Verify by: unit tests for exec rewriting (dev → prod) and paths; integration test of
                 bundle into a tempdir (skipped without deno); live dev loop.

AC-23 As a user, I want the popover to update in place, not flicker or lose what I am doing.
      GIVEN an open popover with a focused Entry mid-word and a Fader being dragged
      WHEN  the applet commits unrelated changes 10 times a second
      THEN  the Entry keeps its text, caret and focus; the drag is not interrupted; only touched
            widgets are re-dressed; an empty-then-filled body becomes visible; closing and
            reopening builds exactly one new shell and the old widgets are freed
      Verify by: the seven exec_dom.rs GTK tests (§6a); caret and drag live (manual list).

AC-19 As an applet author, I want the SDK to speak the documented protocol.
      THEN fixtures match byte for byte, the Rust wire tests decode the same files, and
           console.log goes to stderr
      Verify by: just sdk-test; wire tests.
```

## 10. How the pieces connect

- **Document → service.** `exec::Config::from` → `placed_applets`. `PanelServices::reconfigure`
  (`services.rs:265-286`) gains exec.
- **Panel → service.** `start_with_buses` passes `Arc::new(DesktopCatalog)`, the shared
  `Arc<dyn Selection>` and the notifications handle, and stores the `Running<Exec>` beside the
  others, including in `shutdown` (`services.rs:236-263`).
- **Service → bar.** The `Exec` arm of `applets::build` receives `ExecHandle`, the
  `SessionActionsHandle` and `dialog`. It calls `ctx.watch(handle.subscribe())` and then
  `exec::Exec::start(ctx.name(), …)`.
- **Bar → service.** `spawn_command("exec.send_event", …)` and `exec.popover`.
- **glimpsectl → catalog/systemd.** `exec::catalog`, plus the systemd1 proxies.
- **SDK ↔ service.** Both are pinned by `sdk/applet/fixtures/*.ndjson`.

## 11. Struck

| Considered | Why not |
| --- | --- |
| `applet.toml` / any manifest besides `.desktop` | user: drop it |
| An `X-GlimpseApplet` boolean | `Implements` is the spec's key |
| A panel-built Deno sandbox from config `allow-*` | `Exec` runs as is; `glimpse-applet` holds the flags |
| Validating options | passed through, by user decision |
| A standalone applet host | user decision; the same memory |
| Notification actions and replace-in-place | `Notifications1.Post` lacks both (`notifications.rs:112-136`) |
| Clipboard read, idle inhibit, launching apps by id | not requested |
| `IndicatorSpec.extension`, per-chip pointer routing | a live widget can't cross; `Pointer` has no index |
| `launch_uris_as_manager_with_fds`, `AppInfoMonitor` | pid-only and double-forks; `Retry` covers installs |
| Per-applet `MemoryMax`/`CPUQuota` | one property away on the scope; not asked for |
| `GLIMPSE_APPLET=1` guard, `MAX_DEPTH`, tree coalescing, commented-config entry | ceremony (review) |
| Shipping a pinned deno (~96 MB), embedding rquickjs, embedding deno_runtime | user: deno stays external. A distro package adds `deno` as a dependency of the SDK and launcher |
| AppStream metainfo, JSR | a `.desktop` file + `bundle` output is enough |

## 11a. `.desktop` beyond applets (follow-up beads, not this plan)

Measured today: glimpse ships **no** `.desktop` files. The panel runs as
`app.slice/glimpse-panel.service`, which does not match the portal's `app-…` pattern. GeoClue gets
`DESKTOP_ID="glimpse"` with no matching file.

Follow-ups:
1. `NoDisplay` entries for glimpse's own app ids.
2. systemd-style unit names or aliases.
3. `desktop-id[:action]` in `command`/`settings-command` via `launch_action`.
4. Icon and name resolution through desktop ids for tray, privacy and pager.
5. XDG autostart.

## 12. Open questions — all closed

1. ~~SDK location~~ `sdk/applet/`.
2. ~~Faking notifications~~ a pure `note()` plus `NotificationsProvider::unavailable(r).handle()`.
3. ~~The marker~~ `Implements`.
4. ~~Manifest~~ the `.desktop` file only.
5. ~~Installed after placed~~ `Retry`.

## 12a. Filing (the first thing done after approval)

1. **Worktree.** `EnterWorktree name=external-applets` creates `.claude/worktrees/external-applets`
   on branch `external-applets`.
2. **Durable plan.** Copy this file to `<worktree>/crates/glimpse-services/src/services/exec/DESIGN.md`.
   It is not gitignored, and it is committed only when you ask. Every bead links to it with
   `--spec-id`.
3. **Beads.** Three epics, one per phase, with a child per slice:
   - Epics are created as
     `bd create -t epic "External applets — phase N: <title>" --spec-id <DESIGN.md> --design-file <the phase's §13 table>`.
     Phase 2 is blocked by phase 1, and phase 3 by phase 2.
   - Each child is created as `bd create -t task --parent <epic> --spec-id <DESIGN.md> --body-file <brief>`,
     plus `--deps` on its in-phase prerequisites. The acceptance criteria are copied verbatim into
     `--acceptance`.
   - **Each brief carries exactly the `epic-wave` variables,** so the wave is poured without
     rewriting anything:
     - `exact_paths`;
     - `model_file` (one file with a line range, from §2);
     - `traps` (2–4, from the review record and §3/§3a);
     - `verify_command` (the narrowest `just test-crate <crate>` or `just test-one …`);
     - `live_test_command` (§14, or empty);
     - `ac_list`;
     - `worktree`;
     - `Source: §<section>`, whose names and paths are **copied, not paraphrased**.
   - Before any child is claimed, each brief goes through `spec-precision`'s self-check: every
     named symbol, path and command is confirmed to exist, or to be created by an earlier slice.
4. **Waves.** At execution time, not now, each child gets
   `bd mol pour epic-wave --var task_id=<id> --var …` from its brief. That runs build → review →
   live test → **your merge gate** → merge, one feature at a time per AGENTS.md.

## 13. Phases (slices are created after approval; each carries `Source: <section>`)

Each phase ends in a passing `just verify` and a live check before the next one starts.

### Phase 1 — Preparation

Nothing in this phase is user-visible. It lays the ground the next two phases stand on.

| # | Slice | Crate | Source |
| --- | --- | --- | --- |
| 1.1 | Protocol doc (`sdk/applet/PROTOCOL.md`) + golden fixtures `sdk/applet/fixtures/*.ndjson` (hand-written in this phase) | sdk/applet | §5 Wire, §6 |
| 1.2 | Config: `Exec`/`ExecAppletConfig`, `is_desktop_id`, the `from_name`/`entry` fallback, `placed_applets`, schema regen, README | glimpse-config | §5 Config, AC-2 |
| 1.3 | `DesktopCatalog` + `expand` + refusals (table tests); glimpse-services inherits `gio` | glimpse-services | §3a, §5 Catalog, AC-3 |
| 1.4 | systemd1 proxy additions (`start_transient_unit`, `list_units_by_patterns`, `get_unit_processes`, `Systemd1ScopeProxy`), checked against `busctl --user introspect` | glimpse-dbus | §7 glimpsectl |
| 1.5 | Visibility: `reconcile` module + `by_key` → `pub`; `session::render` → `pub(crate)` | glimpse-widgets, glimpse-panel | §2 |
| 1.6 | Preview `var/widget_examples/exec.blp` + indicator states board, both schemes; **user approves** | var/widget_examples | §8 |

### Phase 2 — Exec applet, tested with a protocol-speaking script

| # | Slice | Crate | Source |
| --- | --- | --- | --- |
| 2.1 | **Example script** `scripts/exec-applet-demo.py` (stdlib Python, no SDK) that speaks the protocol by hand. It sends `Hello`, a chip plus a popover with a Row, a SwitchRow, a Fader and an Entry, and answers events. A Label shows the received placement and options, updated on `placement`/`options`. Clicking rows sends `notify`, `copy`, `open-uri`, `session lock` and `close-popover`. `--mode` switches it into misbehaving: `garbage`, `huge-line`, `flood`, `no-read`, `exit-before-hello`, `crash-after`, `many-nodes`. It comes with `scripts/exec-applet-demo.sh install\|remove`, which writes `~/.local/share/applications/me.aresa.GlimpseExecDemo.desktop` with an absolute `Exec`; both are run by path (no just recipe for a fixture script). The golden fixtures double as its expected output | scripts | §5 Wire, AC-10 |
| 2.2 | Exec service: spawn + adopt, the per-child source, tree validation, generations, status/backoff/retry, attach/place/detach, the per-slot gate, notify/copy, requests; registered in `services.rs` (start, reconfigure, shutdown). Tests use a fake `Catalog` pointing at the demo script | glimpse-services | §5 Service/Spawn, §6, AC-3,4,8,9,10,11,12,14,16 |
| 2.3 | Exec panel applet: the build arm plus `Placement` from `components/panel.rs:282`, slot + attach/`orient`→place/`Drop`→detach, chips (every `IndicatorSpec` field), applet-supplied popover content, **`Dom` reconciliation per §6a** (by_key per container, ptr_eq skip, visibility sync, blocked handlers, caret), `(generation, id, kind)` keys, seq, typing, pointer routing, requests (open-uri with a launch context, session via confirm, close-popover), panic-free signal closures | glimpse-panel | §6, §6a, AC-1,5,6,7,13,15,23 |
| 2.4 | `glimpsectl applets list\|inspect\|restart`; glimpsectl gains `glimpse-services` | glimpsectl, glimpse-services | §7, AC-17, AC-21 |
| 2.5 | Live pass with the demo script (§14, phase 2 list); READMEs; AGENTS.md Known state for what was measured | — | §14 |

### Phase 3 — SDK and a demo applet (todo list)

| # | Slice | Crate | Source |
| --- | --- | --- | --- |
| 3.1 | `glimpse-applet` launcher: argv builder, Deno lookup, exec, errors; added to the justfile `binaries` and the package lists | glimpse-applet | §5 Launcher, AC-18 |
| 3.2 | SDK `sdk/applet/`: reconciler on the §5 wire (speaks `Hello` first), components, host calls, `useOptions`/`usePopoverOpen`, console → stderr, `mod_test.ts` against the phase-1 fixtures, `just sdk-test` | sdk/applet | §7, AC-19 |
| 3.3 | Template files + `glimpsectl applets new` (interview, flags override questions, `--yes`, `render`, escaping; `dialoguer` if approved) | sdk/applet, glimpsectl | §7, AC-20 |
| 3.3b | `glimpsectl applets check\|dev\|bundle\|install\|uninstall`; `glimpse_utils::deno()`; the AGENTS.md home-write exemption | glimpsectl, glimpse-utils | §7, AC-22 |
| 3.4 | **Demo applet: todo list,** `sdk/applet/examples/todo/`. The chip shows the open count as a `badge`, with a `dot` when anything is overdue. The popover has a Section of Rows (activate = done), an Entry (submit = add), a SwitchRow "show done", and a footer Button "copy all" (`copy`). `options.title` is the Section title. On a vertical placement the chip drops its label and keeps
the badge. Items persist in `localStorage`. Each bar runs its own process, and Deno fires no
cross-process storage event, so a second bar sees an addition only on its next start. That is
stated in the demo's README as the known limit of per-bar processes. It ships a `.desktop` with `Exec=glimpse-applet …` | sdk/applet | §3a, §7 |
| 3.5 | Packaging (`sdk/applet` → `DATA_DIR/sdk/applet`, the launcher), AGENTS.md rows (`glimpse-applet` crate, `sdk/`), live pass with the todo applet under `--watch` and bundled | data, packaging | §7, §14 |

## 14. Verification

- `just verify` in the worktree; `just test-crate <crate>` while iterating; `just sdk-test` with
  deno installed.
- **Live setup:** a second panel with `--config <scratch>`, a fresh `GLIMPSE_PANEL_APP_ID`, its log
  outside the scratch directory, on the glimpse workspace. Never `pkill -x glimpse-panel`; kill it
  by its own pid.
- **Phase 2, with `scripts/exec-applet-demo.sh install`, then `center = ["me.aresa.GlimpseExecDemo"]`:**
  - chips appear;
  - the popover opens with only the applet-supplied content;
  - every row acts once;
  - the gated requests work only after a click;
  - `kill -9` on the child → it restarts;
  - editing options → the same pid;
  - alive after 60 s;
  - each `--mode` stops only that child while a second instance keeps updating;
  - `kill -9` on the test panel → no orphan;
  - `glimpsectl applets list` and `inspect` show it, including the scope, pid and memory.
- **Phase 3, with the todo applet:**
  - first under `glimpse-applet --watch`: edit, save, and the change reloads in the open popover;
  - then bundled, under `systemd-run --user`, to prove the Deno lookup.
- `just preview var/widget_examples/exec.blp`, in both schemes.

## 15. Review record

**2026-09-24, user.** The applet must receive its placement, so there is one process per bar
placement. That removed the open counting, the bar ids and the outbox addressing. `Placement` was
added to `Hello` together with an `Outgoing::Placement` update, and the service moved to slots
with `attach`/`place`/`detach`.

**2026-09-24, fresh adversarial reviewer (read-only, against the tree).** It raised 15 defects and
5 taste items. All defects are applied above; the taste items are applied except `list`, which the
user requested.

| Finding | Changed |
| --- | --- |
| The outbox was skipped by the `ptr_eq` return | the bar carries out `requests` first |
| `--watch` reload: no host reply, no generation bump | the applet speaks first; the host answers each `Hello`; `Event::Hello`; generation increases per `Hello`; widgets keyed `(generation, id)` |
| An applet could hold the gate open through dress echoes | `gesture` is set only by clicked/activate/pointer; dressing is quiet |
| Resetting `attempt` respawned healthy children | a monotonic `spawn` in the key; `attempt` computed from `ran` at exit |
| The bar id was undefined | `static AtomicU64`; `serial` is service-global |
| A full stdin queue could wedge the handler | `try_send`, and `Full` kills the child; `Outgoing` is owned |
| `TryExec=deno` vs the systemd PATH | `TryExec=glimpse-applet`; the launcher searches `~/.deno/bin` |
| PDEATHSIG is per thread | spawn on the async task, never in `spawn_blocking`; `Signal::KILL` |
| Signal-closure panics | a panic-free rule plus a NaN test |
| The scope race | adopt before `wait()` (a zombie holds its pid); kept for identity and `inspect` |
| Failed never retried | a `Retry` interval while any instance is Failed |
| Hidden entries were not refused | `is_hidden()` plus reading `Implements` directly |
| Stale carried-over ACs | ACs rewritten in full; AC-12 worded as a UI guard |
| Type gaps | `Outgoing`, `Event::Hello`, `UserEvent`, the `unavailable` path, visibility of `reconcile`/`render`, `ExecAppletConfig`, the geolocation cite, the launch context |
| State before the first event | `initial_state` seeds `Starting`; a missing entry means no chips |

## Appendix A. Tree details (carried from the research design, var/external-applets/design.md §6)


```rust
pub const ROOT: u32 = 0;
pub const MAX_NODES: usize = 300;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tree { pub nodes: HashMap<u32, Node> }

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub element: Element,
    pub children: Vec<u32>,
    pub seq: Option<u64>,
    raw: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity { Info, Warning, Error }

#[derive(Debug, Clone, PartialEq)]
pub enum Element {
    Root, Popover, Footer,
    Indicator(IndicatorProps), Hero(HeroProps), Section(SectionProps), Row(RowProps),
    SwitchRow(SwitchRowProps), Fader(FaderProps), Entry(EntryProps), Placeholder(PlaceholderProps),
    Box(BoxProps), Label(LabelProps), Image(ImageProps), Button(ButtonProps), Switch(SwitchProps),
    Scale(ScaleProps), Spinner, Progress(ProgressProps), Separator(SeparatorProps),
    Unsupported(String),
}

// Every props struct: #[derive(Debug, Clone, Default, PartialEq, Deserialize)]
//                     #[serde(default, rename_all = "camelCase")]
// and every one carries `class_name: Option<ClassName>` (the §12a allowlist as an enum).
pub struct IndicatorProps { pub icon: Option<String>, pub text: Option<String>, pub tooltip: Option<String>,
    pub severity: Option<Severity>, pub attention: bool, pub notice: bool, pub on_press: bool, pub on_scroll: bool }
pub struct HeroProps { pub icon: Option<String>, pub title: Option<String>, pub subtitle: Option<String> }
pub struct SectionProps { pub title: Option<String>, pub count: Option<String> }
pub struct RowProps { pub icon: Option<String>, pub title: Option<String>, pub subtitle: Option<String>,
    pub value: Option<String>, pub selected: Option<bool>, pub busy: bool, pub on_activate: bool }
pub struct SwitchRowProps { pub icon: Option<String>, pub title: Option<String>, pub subtitle: Option<String>,
    pub active: bool, pub busy: bool, pub on_toggle: bool }
pub struct FaderProps { pub icon: Option<String>, pub value: f64, pub maximum: f64, pub floor: f64,
    pub muted: bool, pub on_change: bool, pub on_mute: bool }
pub struct EntryProps { pub placeholder: Option<String>, pub value: String, pub on_change: bool, pub on_submit: bool }
pub struct PlaceholderProps { pub icon: Option<String>, pub title: Option<String>, pub description: Option<String> }
// §12a GTK layer: BoxProps, LabelProps, ImageProps, ButtonProps, SwitchProps, ScaleProps,
// ProgressProps and SeparatorProps carry exactly the props in the §12a table.
```

**Props are merged per key, and none is ever stuck.**

1. A `set` merges into `raw`, and a `null` value deletes the key.
2. `Element` is re-derived from `raw` after every `set`.
3. A key whose value has the wrong type is removed from the map passed to serde and logged once
   per `(node, key)`. The other keys still apply.

**Mapping rules the applet applies:**
- `Row.selected: Some(_)` sets `selectable`.
- `on_activate == false` sets `activatable = false`.
- `Fader.on_mute == false` sets `toggleable = false`.

**Text:**
- Labels, titles, tooltips and the `Unsupported` element name go through
  `glimpse_utils::clean(text, cap)`. Caps: 256 chars, and 64 for element names.
- `Entry.value` only has control and bidi characters stripped and a 4096 cap, **with no
  trimming**. `clean` trims whitespace, which would eat a typed trailing space.
- An `icon` must match `^[A-Za-z0-9_.-]+$`; otherwise it is dropped.

**Numbers:**
- Non-finite `f64`s are dropped as a wrong type, because glib panics on a NaN property write.
- `Scale` requires `min < max`.


## Appendix B. The GTK layer (research design §12a, a user decision)


A small set of generic elements sits beside the glimpse set. Each takes an **allowlist** of
props, and nothing else crosses. It uses no reflection and no GObject property names on the
wire, so renaming a GTK or glimpse property never breaks an applet.

| Element | GTK | Props | Events |
| --- | --- | --- | --- |
| `<box>` | `Gtk.Box` | `orientation` (`"horizontal"`/`"vertical"`), `spacing` (0–24), `homogeneous`, `halign`/`valign`, `hexpand`/`vexpand` | — |
| `<label>` | `Gtk.Label` | children = text (plain, never markup), `wrap`, `xalign`, `ellipsize`, `lines` (≤ 8) | — |
| `<image>` | `Gtk.Image` | `icon` (themed name only), `pixelSize` (8–64) | — |
| `<button>` | `Gtk.Button` | children = label, `icon`, `sensitive` | `onClick` |
| `<switch>` | `Gtk.Switch` | `active`, `sensitive` | `onToggle(active)` |
| `<scale>` | `Gtk.Scale` | `value`, `min`, `max`, `step`, `sensitive` | `onChange(value)` + `seq` |
| `<spinner>` | `Adw.Spinner` | — | — |
| `<progress>` | `Gtk.ProgressBar` | `fraction` (0–1), children = text | — |
| `<separator>` | `Gtk.Separator` | `orientation` | — |

Every element above, and every glimpse element, also takes `className`. It is **one class from
a fixed list**: `dim-label`, `caption`, `heading`, `title-1`…`title-4`, `numeric`, `accent`,
`success`, `warning`, `error`, `flat`, `pill`, `circular`. Anything else is dropped with one log
line. There is no applet CSS.

Still absent: `Gtk.Entry` with `visibility: false` (no secret entry), `Gtk.Picture`/file paths,
`Gtk.DrawingArea`, `Gtk.ScrolledWindow` (a popover never scrolls, per the applet skill), and any
window, dialog or popover element. Generic layout does let an applet build shapes the glimpse
grammar would not. The applet owns all of its popover content.
