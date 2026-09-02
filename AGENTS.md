# glimpse

A desktop shell suite for Wayland compositors, targeting Niri first and Hyprland second: a panel, a
wallpaper renderer and a lock screen. One daemon (`glimpsed`) owns every piece of session state and
every OS integration; the UI binaries are stateless clients that render it over a single Unix
socket. When a rule below does not cover a situation, the deciding question is usually "who
owns this state?" — and the answer is almost always the daemon.

## Prior art

This is the second generation of an application that already exists. Two bodies of earlier work are
in the tree, and both are **reference only — never a source of truth.**

| Where          | What it is                                                  |
| -------------- | ----------------------------------------------------------- |
| `_old/`        | the shipped previous implementation, in Rust                |
| `var/glimpse2` | design documents for this rewrite, written by another agent |

Read either when you want to know how a problem was solved before, what a backend actually does, or
which edge cases turned out to matter in practice. That knowledge is the reason they are kept: it
was paid for once already.

Neither one decides anything. `_old/` is one answer among several and frequently the wrong one.
`var/glimpse2` was written without the constraints in this file and does not know what has been
decided since; treat it as a proposal from someone who has left the project.

**Never edit either, never build them, and never copy code out of them.** This is not a port. The
job is a smaller, simpler, cleaner application than the one in `_old/` — if a design lands at the
same size and shape as its predecessor, that is a signal to look again, not a sign of fidelity.

## Structure

```
glimpse/
├── crates/       all Rust code, flat, one directory per crate
├── data/         installed assets: systemd units, D-Bus service files, pam.d, GeoClue policy, default config
├── scripts/      install, uninstall and packaging scripts, plus development helpers; not installed
├── wallpapers/   bundled wallpapers
├── var/          scratch, not installed; `var/glimpse2` holds third-party design drafts
└── _old/         the previous implementation, kept for reference only
```

| Crate                 | Role                                                                              |
| --------------------- | --------------------------------------------------------------------------------- |
| `glimpse-ipc`         | wire frames, codec, errors, client, server                                        |
| `glimpse-contracts`   | `Message` and `Command`, every topic and command payload                          |
| `glimpse-dbus`        | D-Bus proxies and the shared bus connections                                      |
| `glimpse-config`      | layered TOML load, drop-ins, merge, validate, watch                               |
| `glimpse-compositors` | niri and Hyprland IPC: snapshot, events, keyboard/workspace/window/output control |
| `glimpse-services`    | service framework and every service implementation                                |
| `glimpse-widgets`     | GObject subclasses, Blueprint templates, shared CSS                               |
| `glimpse-utils`       | shared CLI arg structs and tracing/log setup used by every binary                 |
| `glimpsed`            | broker, `WaylandEdge` impl                                                        |
| `glimpse-panel`       | panel and applets                                                                 |
| `glimpse-wallpaper`   | background layer surface, decode cache, transitions                               |
| `glimpse-lock`        | `ext-session-lock-v1` surfaces, PAM                                               |
| `glimpse-sunset`      | night-light service                                                               |
| `glimpsectl`          | CLI and TUI                                                                       |

## Stack

- Rust, edition 2024, `rust-version = "1.93"`, one workspace with `members = ["crates/*"]`
- tokio for the daemon; one task per service, handlers run serially on `&mut self`
- zbus for D-Bus, both client and object-server sides
- GTK4 + libadwaita + relm4 + gtk4-layer-shell for UI; Blueprint templates compiled by `build.rs`
- serde and serde_json for the wire protocol — newline-delimited JSON over a Unix socket
- `just` for task recipes

## Conventions

Path-scoped rules load automatically when the relevant files are opened:
`.claude/rules/daemon.md` for `glimpsed`, `glimpse-services` and `glimpse-ipc`;
`.claude/rules/ui.md` for the GTK crates. Writing or changing a service — the `Service` trait, `Ctx`
sources, subscriptions, topics and commands, registration, headless tests — is covered by the
project-local `service` skill in `.claude/skills/service/`. Its panel counterpart is the `applet`
skill, for the `Applet` trait, `Ctx` sources, pull-based indicators and the registration match;
`widget` covers GObject subclasses, Blueprint templates and the three places a new template must be
registered; `ipc-client` covers holding a `Client` from outside the daemon, where a request issued
while `glimpsed` is unreachable fails rather than queues; and `testing` covers which tier a
test belongs to, why GTK tests are one `#[ignore]`d function per crate, and the mutation check that
decides whether an assertion is load-bearing. General GTK4, libadwaita and relm4 craft
is covered by the `relm4`, `gtk4-styles` and `libadwaita-styles` skills. D-Bus work — every mirror service, plus the
two names glimpsed owns — is covered by the project-local `zbus` skill in `.claude/skills/zbus/`,
which carries introspected signatures for NetworkManager, BlueZ, logind, UPower, MPRIS,
StatusNotifierItem, dbusmenu and Notifications.

**Code**

- **Write no comments unless asked.** Not doc comments, not rationale, not a note on a subtle
  branch. Name functions, types and variables so the code reads without one, and put everything a
  comment wanted to say in the crate's `README.md`, where it is found by someone looking for it.
  This overrides any habit of explaining a decision in place — if a decision needs explaining, the
  README is where it goes. Comments already in the tree stay; leave them alone unless the code
  under them changes, and delete rather than update one that has stopped being true.
- **US English.** `color`, not `colour`, in identifiers, comments and user-facing strings.
- Every binary fails the same way: `run(cli) -> anyhow::Result<()>` does the work and `main` turns
  the outcome into an `ExitCode`, because `?` cannot be used in a function returning one.
  `errors.rs` holds both halves — a private module of named code constants for that binary's exit
  codes, and the single `exit_code(&anyhow::Error) -> ExitCode` that maps them by
  `downcast_ref`. One mapping site is what stops a `.context(...)` added upstream from changing
  which code a script sees. `ExitCode` is opaque, so split the `u8` out to keep it testable.
- User-facing strings are not comments. `help = "..."` on a clap argument, an error message, a log
  line — all fine, and stripping them breaks output.

**Dependencies**

- Every crate dependency is inherited: `serde.workspace = true`. Add the version to
  `[workspace.dependencies]` in the root `Cargo.toml`, never to a crate manifest.
- A dependency belongs in `glimpse-ipc` only if both ends of the socket need it, plus `tracing` for
  diagnostics. Payloads live in `glimpse-contracts`, bound to their names by `trait Message` and
  `trait Command`; no zbus, no GTK, no backend type reaches either. `glimpse-contracts` and
  `glimpse-ipc/src/frame.rs` are the input to `schemars` for the Python, TypeScript and Go SDK
  types.
- Errors: `thiserror` in a library, whose caller must branch on the failure; `anyhow` in a binary,
  where every failure ends at one message and one exit code.
- Nothing depends on `glimpsed`. It is a leaf. Shared code goes in proto, client, config, services
  or widgets.
- A trait the framework needs from the daemon is declared in `glimpse-services` and implemented in
  `glimpsed` — `BrokerHandle`, `WaylandEdge` — each with a mock beside the declaration.

**Naming**

- Topics are `domain.name`, lower snake case, dots as separators: `audio.volume`,
  `tray.item.{id}.menu`
- Commands are `domain.verb_object`: `audio.set_volume`, `tray.menu_about_to_show`
- **Never prefix a type with `Glimpse`.** A GObject type name and the blueprint template that binds
  to it are `Hero`, `PopoverShell`, `Panel`, `IndicatorGroup` — the crate already says whose they
  are, and the prefix only makes every name longer than the thing it names. It survives solely where
  a reverse-DNS identifier demands it: application IDs, D-Bus names, the gresource path.
- **Never build a glimpse path by hand.** `glimpse-config` owns where glimpse files live and
  exports it: `user_dir()` for `~/.config/glimpse`, `DATA_DIR` for `/usr/share/glimpse`. Writing
  `dirs::config_dir().join("glimpse")` or `"/usr/share/glimpse/..."` in another crate makes a second
  answer to a question that already has one — it was duplicated four ways before this rule existed.
  A user-overridable file is looked up in `user_dir()` first, then `DATA_DIR`.
- One config file, `config.toml`. Top-level table per owner: one table per service, named for the
  service, for the daemon; `[panel]`, `[wallpaper]`, `[lock]` for the UI
  binaries. A binary reads only the tables it owns. Stylesheets stay separate: `panel.css`,
  `lock.css`.

**File placement**

| Kind of file                                                            | Goes in                          |
| ----------------------------------------------------------------------- | -------------------------------- |
| wire payload type                                                       | `glimpse-contracts/src/`         |
| service implementation                                                  | `glimpse-services/src/services/` |
| anything touching a `wl_` object                                        | `glimpsed/src/wayland/`          |
| anything touching GTK                                                   | a UI crate or `glimpse-widgets`  |
| systemd unit, D-Bus service file, pam.d entry, GeoClue policy, defaults | `data/`                          |

**Services**

- Mirror services (network, bluetooth, audio, battery, mpris, brightness) enumerate once at start
  then follow change signals. The backend is right when they disagree.
- Never reimplement a decision the backend already makes — no auto-connect policy, no reconnect
  loops, no retry logic on top of NetworkManager.
- Commands are thin pass-throughs to the backend.
- A handler that can block moves its `Responder` into `ctx.spawn`. Handlers run serially, so one
  slow D-Bus call otherwise freezes the whole service.

**UI**

- An applet renders topics and sends commands. It never opens a D-Bus connection, never reaches a
  backend directly, and holds no state that outlives its own widget.
- UI state never waits on a round trip. Update the widget optimistically and let the topic event
  reconcile it.
- A widget moves to `glimpse-widgets` as soon as a second binary needs it.

## Verification

`just` is the only entry point. Run `just` with no arguments to list recipes.

```bash
just verify          # fmt-check + check + lint + test — what CI runs
just check           # type-check, fast
just lint            # rust, systemd units and blueprints, warnings are errors
just test            # headless tests
just fmt             # format in place
just test-compositor # also runs the #[ignore] Wayland tests; needs a compositor
just check-units     # systemd-analyze verify on the shipped units
```

Running a binary goes through `just run-daemon`, `just run-panel`, `just run-wallpaper`,
`just run-locker`, `just ctl <args>`. `just nested` opens a nested niri
window for a dev loop that does not disturb the running session.

A recipe that is missing or wrong gets fixed in the `justfile`. Do not work around it with a raw
cargo invocation.

**Previewing a widget.** `just preview <blueprint.blp>` renders one blueprint with the **real
widgets** and reloads it whenever the blueprint or the theme is saved. It is a cargo example in
`glimpse-widgets`, so it links the crate: `Gtk.Builder` resolves `$PopoverShell` and `$Hero` to the
Rust types, not to look-alikes. `Esc` closes it.

Four things it must do, every one of which fails silently otherwise:

- **Touch every widget type before building.** A Rust GType registers lazily, so a `$Hero` that
  nothing has instantiated is simply an unknown class to `Builder`. `ensure_types` names them all.
- **Take `ApplicationFlags::NON_UNIQUE`.** The application ID is on the session bus, which is shared
  across displays — a second preview otherwise hands off to the first, which may be on another
  monitor or in another compositor, and exits 0 with no window and no message.
- **Read `glimpse.css` from disk, not through `Styles::install`.** That loads the sheet with
  `include_str!`, so a preview built on it renders a *compiled-in copy* and no edit can ever reach
  it. The preview installs its own providers at the same priorities, which also means a deleted rule
  actually disappears.
- **Spell a watched path the way the file monitor spells it back.** A relative argument or a `..`
  component compares unequal to the absolute, resolved path GIO reports, so every event is
  discarded. `resolve` canonicalises both the blueprint and each stylesheet. This is the same defect
  that made `glimpse-config`'s watcher silently dead, in a different library.

Live reload watches each file's **directory**, not the file, and treats a rename onto the path as a
change. An editor that saves by writing a temporary file and renaming it over the original destroys
the inode a file monitor holds, and GIO then reports the write on a two-second timer — which is what
a laggy reload actually is. The rename arrives as `RENAMED`, whose *first* argument is the temporary
path and whose `other_file` is the one you asked for, so matching only the first argument ignores
every such save. Both paths are checked, and events are coalesced over 40ms.

The window paints a checkerboard and every child stays transparent, so whatever the widget does not
paint reads as pattern rather than as a flat background it never asked for. That is a diagnostic:
libadwaita's `.card` is an 8% white overlay in dark mode, and against a plain window it looks solid.

**Both halves of the checkerboard rule must be scoped to `window.preview`, not just the first.**
GTK4 parents a tooltip, a popover and a drag icon *into the widget tree*, as direct children of the
window — measured: a `Gtk.Popover` given `set_parent(window)` appears in that window's child list
beside its `child`. So a companion `window.preview > * { background-color: transparent; }` matches
every one of them, at `STYLE_PROVIDER_PRIORITY_USER + 2` against libadwaita's `tooltip.background`
at `THEME`, and blanks it. The symptom is a tooltip with no background at all, in a preview whose
checkerboard rule already looks correctly scoped. The transparency rule names the preview's own slot
instead.

`--scheme dark` (or `light`) forces the color scheme; without it the preview follows the system. The whole token vocabulary flips at once, so a widget is not checked until it has been
seen under both.

It opens floating, through a `window-rule` on `^me\.aresa\.WidgetPreview` in the niri config. A
preview is one widget sized to itself, and tiling it into a column tells you nothing about how it
looks. Layer-shell was tried first and rendered nothing.

Some widgets cannot be filled from a `.blp` at all, because their data is not a property — a
calendar's events are a list of colours per day. `just preview <blp> [fixture]` runs a **named
fixture** over the built tree, and the name defaults to the blueprint's own stem, so
`calendar.blp` shows sample events by being opened rather than by being opened with an argument
nobody knows about. The fixtures live in the preview example, not in the widgets.

**An example may carry its own stylesheet, and its directory carries a shared one.** `just preview
<name>.blp` loads and watches `_shared.css` beside it at `STYLE_PROVIDER_PRIORITY_USER + 1` and
`<name>.css` at `USER + 2`, and silently loads nothing when either is absent; the checkerboard sits
above both at `USER + 3`. This is what keeps demo-only rules — a weather range bar, a color swatch
— out of `glimpse-widgets/styles/glimpse.css`, which is the shipped sheet and not a scratchpad.

The shared sheet is what the per-example one could not be. Every popover example needs the same
column and drawer floors, and `width-request: 400` was that floor written as a pixel literal
`ui.md` forbids — but seventeen copies of `.column { min-width: 25rem }` is the drift a shared file
exists to prevent, and shaping the width would mean editing seventeen files. `_shared.css` holds
`.column`, `.drawer-page`, `.block`, `.caption`, `.slider` and `.mute`; a name that means something
in exactly one example (`.swatch--e0563f`) stays in that example's own sheet.

`var/widget_examples/` holds whole compositions — `popover_shell_full.blp` is a popover with every
slot filled, and there is one per applet popover. An example is a top-level object, not a
`template`: `Builder` cannot instantiate a template whose class does not exist, so a `template`
root renders as nothing at all. `just check-examples` compiles every one of them, which the
preview otherwise only does one at a time.

**A row followed by a `Gtk.Revealer` expands it.** `fixtures::expanders` gives any row carrying
`.expander` a click handler toggling its **next sibling**, and complains on stderr when that
sibling is not a `Gtk.Revealer`. Matching on position rather than on a name is what keeps it out of
the blueprint: the row and the thing it reveals are already siblings in the section's box, so there
is nothing to keep in sync. `Row`'s rule is still that it navigates rather than expands — this is
the exception the README names, an audio stream revealing its volume slider.

**The drawer is wired for every example, not a named list.** `fixtures::apply` runs its named
fixture and then calls `drawer_nav` unconditionally, which returns immediately when the tree holds
no `Gtk.Revealer`. So a popover written entirely in Blueprint — a `Revealer` holding a `Gtk.Stack`,
and rows carrying `nav__<page>` — navigates with no Rust at all. Gating it on a match arm is what
made a new example's drawer silently inert, and it is the same defect as a `nav__` class with no
page behind it, which `drawer_nav` now reports on stderr.

**A widget is only declarable if it says so.** `PopoverShell` and `Hero` implement `Gtk.Buildable`,
which is what makes `[hero]`, `[footer]` and `[slot]` land in the right internal box, and `Hero`
exposes `title`, `subtitle` and `icon-name` as properties so a `.blp` can set them through the same
capped setters Rust uses. Without both, a `.blp` can name the type and nothing else, and the only
way to preview a composition is to hand-copy its structure — which is a copy, not the widget.

`add_child` must ignore the widget's **own** template children. `init_template` adds them through
the very interface being overridden, so an unguarded override routes `hero_box` into `content_box`
and panics on an unbound `TemplateChild` before the widget exists. The guard is
`self.content_box.try_get().is_none()`.

**PyGObject cannot do this.** It cannot override an interface vfunc that the parent already
implements — `Gtk.Widget` implements `Gtk.Buildable`, so a `do_add_child` on a Python subclass is
accepted, never called, and children land wherever the default put them. Measured, not assumed. Any
preview host that needs real widgets has to be Rust.

**`blueprint-compiler lint` false-positives on every `$CustomType` it cannot resolve.** It reports
`scrollable_parent` — "Scrollable widget should be placed in a scroll container" — for any extern
type inside a container, verified with a `$Foo` that does not exist. There is no way to exclude a
single rule (`-c`/`-r` are allowlists, and an unknown category silently lints nothing), so the
`lint-blueprints` recipe strips ANSI colour from the report and fails only on a `warning:`/`error:`
line that is *not* `scrollable_parent`. **Embed our own widgets declaratively** — `$RangeBar bar {}`
inside `forecast_day.blp`, `$Scrubber scrubber {}` inside `now_playing.blp` — and bind them as
ordinary `TemplateChild`s. Working around the linter instead costs a compile-checked child and buys
a runtime `expect()`; the recipe is where a broken tool gets handled.

An embedded type needs **no** `ensure_type()` call: `#[template_child] TemplateChild<Scrubber>`
names the type in Rust, and binding it registers the GType before `init_template` resolves the
class by name. Measured both ways in a fresh process. The preview still needs its own
`ensure_types()`, because a blueprint *example* names `$Scrubber` with nothing in Rust touching it
at all — that is the case where lazy registration actually bites.

**`blueprint-compiler lint` also rejects a `Gtk.Adjustment` carrying anything besides `lower`,
`upper` and `value`.** It reports `adjustment_prop_order` — "properties should be ordered as lower,
upper, and then value" — but the order is not what it checks: measured, `lower/upper/value` passes
and adding `step-increment` fires it regardless of position. Set the increments from Rust, and
assert them, because nothing in the template guards them any more.

**Never run a test against the live configuration.** `~/.config/glimpse/config.toml` is the user's
own, it is edited outside this repository, and a daemon started without `--config` both reads it and
watches it. Point every run at a scratch file instead:

```bash
glimpsed --config "$SCRATCH/config.toml"          # replaces the whole stack, drop-ins included
HOME="$SCRATCH/home" glimpsed                     # a fake home, when drop-ins are the thing under test
```

`--config` is the default choice and is enough for anything that is one document. It cannot exercise
layering, because an explicit path replaces the stack rather than joining it — so a test that needs
`config.d/` sets `HOME` (or `XDG_CONFIG_HOME`) to a directory built for the test and lets `user_dir()`
resolve into it. Neither redirects the `/etc/glimpse` layer; a test that needs that layer builds it
through `load_from`, which takes the system directory as an argument for exactly this reason.

**Themes are redirected separately.** `--config` does not move them, because `theme_dir_for` resolves
through `user_dir()` rather than through the configuration stack. `GLIMPSE_THEMES_DIR` names a themes
root and replaces both the user and the installed one, for loading and for watching alike, so a theme
test needs neither a fake `HOME` nor a writable `/usr/share/glimpse`. `GLIMPSE_THEME` overrides the
selected name the same way, so a run can be pointed at a theme without writing a `config.toml` at
all.

**Send the log somewhere else as well.** `--config` watches that file's _parent directory_, so a run
that redirects the daemon's output into it makes every line the daemon writes an event that makes it
read the configuration again. With a document that will not parse, that is a closed loop running at
exactly `DEBOUNCE` — it looks precisely like a watcher retrying, and it is not.

`scripts/` still holds helpers written. Several are useful as-is —
`mpris-fake-players.py`, `network-test-fixtures.sh`, the `privacy-test-*` probes, and
`glimpse-lock-rescue-pam.sh`.

## Critical constraints

- **Never add `panic = "abort"` to any profile.** Per-service panic isolation depends on unwinding;
  abort turns one bad handler into a dead daemon and takes tray and notifications down with it.
- **Never touch a `wl_` object outside `glimpsed/src/wayland/`.** Services reach Wayland only through
  `trait WaylandEdge`, which is what keeps every service test headless.
- **`_old/` and `var/glimpse2` are reference only.** Never edit them, never build them, never copy
  code out of them. See Prior art.
- **Never sandbox `glimpse-lock.service`.** `NoNewPrivileges=`, `PrivateUsers=`,
  `RestrictSUIDSGID=` and anything implying them strip setuid from `unix_chkpwd`. PAM then returns
  `AUTHINFO_UNAVAIL`, the correct password is rejected, and the session cannot be unlocked. The
  symptom looks like a wrong password, which is what makes it expensive to diagnose.
- **No `unwrap()`, `expect()`, or blocking calls in the broker or a service handler.** A panic in
  the broker kills every client's connection; blocking `std::fs`, `Command::output()`, or a
  `std::sync::Mutex` held across `.await` stalls delivery for everyone.
- **Never shell out to `systemctl`, `loginctl`, `nmcli`, `bluetoothctl`, or `niri msg`.** Use D-Bus
  or the compositor's IPC socket. Subprocesses cannot be mocked in tests, break under sandboxing,
  and parse output that is not a stable interface.
- **glimpsed writes runtime state under `$XDG_RUNTIME_DIR/glimpse/` and nothing else.** Never
  `$XDG_CONFIG_HOME`, never the user's home, never `/tmp`.
- **Treat text from other applications as hostile.** Tray titles, notification summaries and bodies,
  MPRIS metadata and SSIDs are attacker-controlled and unbounded. Cap length, ellipsize, and
  sanitize markup before any of it reaches a label.
- **No unit relationship may stop `glimpse-lock.service` while it holds the lock.** A stopped locker
  is a locked session with nothing left to authenticate against, not an unlocked one. `PartOf=` on
  anything but `graphical-session.target`, `BindsTo=`, or a `Conflicts=` from someone else's target
  all reach that state; `Wants=`/`WantedBy=` cannot.
- **No unit may use `Requires=glimpsed.service`.** `Wants=` only — the panel, wallpaper and lock must
  survive a dead daemon, and `Requires=` kills them instead.
- **Never hand-roll what a library already does.** Search in this order and stop at the first hit:
  the standard library, then a crate already in `[workspace.dependencies]`, then a crate that exists
  on crates.io. Writing it yourself is the last resort, not the default. Before adding a `fn` that
  parses, formats, resolves, encodes or retries anything, read the root `Cargo.toml` — the answer is
  often already declared and unused. `XDG_RUNTIME_DIR` resolution was written out longhand here
  while `dirs` sat in the workspace doing exactly that.
- **Propose a new dependency, never add one silently.** If nothing in `std` or the workspace fits,
  name the crate, say what it replaces and how much code that saves, and wait. Adding a dependency
  is the user's call; writing forty lines to avoid asking is not a way around that.
- **Always the current latest version, and confirm it before it lands.** Look the version up —
  `cargo search`, `cargo add --dry-run`, the registry — rather than recalling one. A remembered
  version number is usually a year stale and resolves against an API that has since moved, which
  surfaces as compile errors nobody expected from a line they did not write. Name the exact version
  in the proposal and wait for confirmation before adding it to `[workspace.dependencies]`.
- **Use `just`, never raw `cargo`.** Fix or add a recipe rather than working around a missing one.
- **Never hand work back without running the pass in Finishing.** Every time, before saying
  anything is done.
- **Do not commit or push without being asked.**

## Known state

Facts measured about code that would otherwise invite rework. Each says what was counted and when,
so a later reader can tell a finding from an opinion.

**`glimpse-config/src/watch.rs`, August 2026.** 347 production lines against 391 of tests. `Watch`
and `Arm` — arming inotify, falling back onto an ancestor when `config.d/` does not exist, re-arming
when a directory is replaced under it — are **44%** of the production half. That bulk is one design
decision's consequence rather than accident: four directories are watched and two of them usually do
not exist. Every branch of it has a test, and it has not been the source of a bug.

What is genuinely dead, and is the cut to make in whichever change next touches the file:

- `Update`, `watch` and `watch_all` are `pub` and re-exported from `lib.rs`, and have **no consumer
  anywhere outside this module's own tests**. Every binary in the tree reloads through
  `watch_config`. Verified by grep across `crates/`; the apparent hits are solar's own
  `Event::Update` and `watch_config` imports.
- `watch_config` is therefore the only reader of `Update`, and it discards `Changed`'s
  `Vec<PathBuf>` and treats `Changed` and `Rearmed` as one arm. The only distinction the tree draws
  is "something happened" against "the watch is dead" — so those paths are collected in `forward`,
  carried through the channel and filtered in `Watch::next` to build a value nobody reads.

Both defects found in this file in August 2026 were in the _simple_ 18% that decides whether to
reload and whether to complain, or in the harness testing it — not in the machinery. Its size is not
by itself a reason to rewrite it. Bead `glimpse-aqi5` records the one limitation the design knowingly
accepts.

## Finishing

Finishing is a pass over the work, not the moment the last edit compiles. Run it every time, before
saying anything is done.

1. **Formatter and linter clean.** `just fmt`, then `just lint`, and `just verify` when code
   changed. Zero errors and zero warnings — clippy runs with `-D warnings`, so a warning left behind
   is a broken build for whoever runs it next, not a cosmetic note. Silencing one with
   `#[allow(...)]` rather than fixing it needs a reason worth saying out loud.
2. **Delete what nothing calls.** Dead functions, unused constants, a type kept "for later", a
   wrapper whose body is a single call, a trait with one implementation. Anything that earns its
   place only in an imagined future has not earned it; add it back in the change that needs it.
3. **Cut the ceremony.** A custom error type carrying no information a message would not, a builder
   for two fields, a helper called once, a test asserting that the standard library works.
4. **Check the docs the change invalidated.** The crate `README.md` first — a stale README is worse
   than none, because it is believed.
5. **Read it as a stranger would.** Would you put this in front of someone whose opinion you value?
   If any part of it would need an apology, that part is the finding.

**Findings are work, not notes.** Fix them and run the pass again. It ends when a full pass turns up
nothing, not when the list gets short.

## Keep the documentation current

These are not chores to batch up later. A stale document produces confidently wrong work, which
costs more than the document saved.

- **Update this file whenever you learn something that would change how the next agent works.** A
  non-obvious gotcha, a command that turns out to be wrong, a convention discovered in the code, a
  tool that does not behave as documented, a constraint that stopped being true. If you spent time
  finding it out, write it down here.
- **Update the crate's `README.md` in the same change that alters what the crate does.** Each one
  states purpose, contents, and the rules specific to that crate. New module, changed rule, moved
  responsibility — all of it lands in the README alongside the code.
- Remove instructions that stop being true rather than adding a caveat beside them. Two rules on the
  same topic produce worse behaviour than one.

## Other rules

- spawn desktop windows on `glimpse` niri workspace, do not steal focus

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:970c3bf2 -->

## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/SYNC_CONCEPTS.md for details and anti-patterns.

## Agent Context Profiles

The managed Beads block is task-tracking guidance, not permission to override repository, user, or orchestrator instructions.

- **Conservative (default)**: Use `bd` for task tracking. Do not run git commits, git pushes, or Dolt remote sync unless explicitly asked. At handoff, report changed files, validation, and suggested next commands.
- **Minimal**: Keep tool instruction files as pointers to `bd prime`; use the same conservative git policy unless active instructions say otherwise.
- **Team-maintainer**: Only when the repository explicitly opts in, agents may close beads, run quality gates, commit, and push as part of session close. A current "do not commit" or "do not push" instruction still wins.

## Session Completion

This protocol applies when ending a Beads implementation workflow. It is subordinate to explicit user, repository, and orchestrator instructions.

1. **File issues for remaining work** - Create beads for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **Handle git/sync by active profile**:
   ```bash
   # Conservative/minimal/default: report status and proposed commands; wait for approval.
   git status

   # Team-maintainer opt-in only, unless current instructions forbid it:
   git pull --rebase
   bd dolt push
   git push
   git status
   ```
5. **Hand off** - Summarize changes, validation, issue status, and any blocked sync/commit/push step

**Critical rules:**

- Explicit user or orchestrator instructions override this Beads block.
- Do not commit or push without clear authority from the active profile or the current user request.
- If a required sync or push is blocked, stop and report the exact command and error.

<!-- END BEADS INTEGRATION -->

<!-- BEGIN BEADS CODEX SETUP: generated by bd setup codex -->

## Beads Issue Tracker

Use Beads (`bd`) for durable task tracking in repositories that include it. Use the `beads` skill at `.agents/skills/beads/SKILL.md` (project install) or `~/.agents/skills/beads/SKILL.md` (global install) for Beads workflow guidance, then use the `bd` CLI for issue operations.

### Quick Reference

```bash
bd ready                # Find available work
bd show <id>            # View issue details
bd update <id> --claim  # Claim work
bd close <id>           # Complete work
bd prime                # Refresh Beads context
```

### Rules

- Use `bd` for all task tracking; do not create markdown TODO lists.
- Run `bd prime` when Beads context is missing or stale. Codex 0.129.0+ can load Beads context automatically through native hooks; use `/hooks` to inspect or toggle them.
- Keep persistent project memory in Beads via `bd remember`; do not create ad hoc memory files.

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/SYNC_CONCEPTS.md for details and anti-patterns.
<!-- END BEADS CODEX SETUP -->
