# glimpse

A desktop shell suite for Wayland compositors, targeting Niri first and Hyprland second: a panel, a
wallpaper renderer and a lock screen. There is no daemon: `glimpse-panel` owns its services in its
own process, and notifications, weather and the night light are standalone providers that own theirs
behind a typed D-Bus name each. When a rule below does not cover a situation, the deciding question
is usually "who owns this state?" — and the answer is the one process whose name is on it.

## Prior art

`_old/` is the shipped previous implementation in Rust; `var/glimpse2` is design drafts for this
rewrite by another agent. Both are **reference only — never a source of truth.** Read them for how a
problem was solved or which edge cases mattered; that knowledge was paid for once already.

**Never edit either, never build them, never copy code out of them.** This is not a port. The job is
a smaller, simpler application than `_old/` — a design that lands at the same size and shape as its
predecessor is a signal to look again, not a sign of fidelity.

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

| Crate                   | Role                                                                              |
| ----------------------- | --------------------------------------------------------------------------------- |
| `glimpse-dbus`          | D-Bus proxies and the shared bus connections                                      |
| `glimpse-config`        | layered TOML load, drop-ins, merge, validate, watch                               |
| `glimpse-compositors`   | niri and Hyprland IPC: snapshot, events, keyboard/workspace/window/output control |
| `glimpse-services`      | service framework and every service implementation                                |
| `glimpse-widgets`       | GObject subclasses, Blueprint templates, shared CSS                               |
| `glimpse-utils`         | shared CLI arg structs, tracing/log setup, gettext binding and text cleaning      |
| `glimpse-panel`         | panel and applets                                                                 |
| `glimpse-notifications` | notification owner, typed D-Bus provider and transient popup layer surface        |
| `glimpse-weather`       | weather provider                                                                  |
| `glimpse-wallpaper`     | background layer surface, decode cache, transitions                               |
| `glimpse-lock`          | `ext-session-lock-v1` surfaces, PAM                                               |
| `glimpse-sunset`        | night-light service                                                               |
| `glimpsectl`            | CLI and TUI                                                                       |
| `glimpse-package`       | the suite's `.deb`/`.rpm` manifest; no code                                       |

## Stack

- Rust, edition 2024, `rust-version = "1.93"`, one workspace with `members = ["crates/*"]`
- tokio; one task per service, handlers run serially on `&mut self`
- zbus for D-Bus, both client and object-server sides
- GTK4 + libadwaita + relm4 + gtk4-layer-shell for UI; Blueprint templates compiled by `build.rs`
- serde and serde_json
- `just` for task recipes

## Skills and rules

Path-scoped rules load automatically: `.claude/rules/daemon.md` for `glimpse-services` and the
standalone providers, `.claude/rules/ui.md` for the GTK crates.

| Skill          | Covers                                                                         |
| -------------- | ------------------------------------------------------------------------------ |
| `service`      | the `Service` trait, `Ctx` sources, subscriptions, topics, commands, registration |
| `applet`       | the `Applet` trait, pull-based indicators, registration match, popovers        |
| `widget`       | GObject subclasses, Blueprint templates, the three registration points         |
| `testing`      | which tier a test belongs to, the `#[ignore]`d GTK tests, the mutation check   |
| `live-testing` | a compositor run: isolated socket, scratch config, a second panel              |
| `zbus`         | every mirror service and the provider names, with introspected signatures      |

General craft lives in the `relm4`, `gtk4-styles` and `libadwaita-styles` skills.

## Conventions

**Code**

- **Write no comments unless asked.** Not doc comments, not rationale, not a note on a subtle
  branch. Name things so the code reads without one, and put what a comment wanted to say in the
  crate's `README.md`. Comments already in the tree stay; delete rather than update one that has
  stopped being true.
- **US English.** `color`, not `colour`, in identifiers, comments and user-facing strings.
- Every binary fails the same way: `run(cli) -> anyhow::Result<()>` does the work and `main` turns
  the outcome into an `ExitCode`. `errors.rs` holds a private module of named exit-code constants
  and the single `exit_code(&anyhow::Error) -> ExitCode` that maps them by `downcast_ref`. One
  mapping site is what stops an upstream `.context(...)` from changing which code a script sees.
  `ExitCode` is opaque, so split the `u8` out to keep it testable.
- User-facing strings are not comments. Clap `help`, error messages and log lines all stay.

**Dependencies**

- Every crate dependency is inherited: `serde.workspace = true`. Versions go in
  `[workspace.dependencies]` in the root `Cargo.toml`, never in a crate manifest.
- No contract type derives `JsonSchema` — `schemars` is inherited only by `glimpse-config`.
- **A workspace dependency nothing uses yet is unverified.** Cargo does not resolve features for an
  entry no crate inherits, so a wrong feature name looks correct until the first `workspace = true`
  that names it. Expect a declared-and-unused dependency's features to be a version stale.
- Errors: `thiserror` in a library whose caller must branch; `anyhow` in a binary, where every
  failure ends at one message and one exit code.
- A binary crate is a leaf: nothing depends on one. Shared code goes in config, dbus, services,
  compositors, utils or widgets.
- The dependency order is one-way — `glimpse-services` depends on `glimpse-dbus`, never the reverse.
  Anything a provider decodes off the bus lives in `glimpse-dbus` beside its decoder; everything
  else sits beside the service that owns it.

**Naming**

- Commands are `domain.verb_object`: `audio.set_volume`, `tray.menu_event`. This is the label
  passed to `spawn_command`; there are no topic names, because state reaches an applet as one typed
  `watch::Receiver` rather than as a named payload.
- **Never prefix a type with `Glimpse`.** Types are `Hero`, `PopoverShell`, `Panel`,
  `IndicatorGroup` — the crate already says whose they are. The prefix survives only where a
  reverse-DNS identifier demands it: application IDs, D-Bus names, the gresource path.
- **Never build a glimpse path by hand.** `glimpse-config` owns where glimpse files live:
  `user_dir()` for `~/.config/glimpse`, `DATA_DIR` for `/usr/share/glimpse`. A user-overridable file
  is looked up in `user_dir()` first, then `DATA_DIR`.
- One config file, `config.toml`, with a top-level table per owner: one per service, plus `[panel]`,
  `[wallpaper]` and `[lock]`. A binary reads only the tables it owns. Stylesheets stay separate:
  `panel.css`, `lock.css`.

**File placement**

| Kind of file                                                            | Goes in                          |
| ----------------------------------------------------------------------- | -------------------------------- |
| provider wire type and its decoder                                      | `glimpse-dbus/src/clients/`      |
| service implementation                                                  | `glimpse-services/src/services/` |
| anything touching a `wl_` object                                        | the owning UI or compositor crate |
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

`just` is the only entry point; run it with no arguments to list recipes. A recipe that is missing
or wrong gets fixed in the `justfile` — never worked around with a raw cargo invocation.

```bash
just verify          # fmt-check + check + lint + test — what CI runs
just check           # type-check, fast
just lint            # rust, systemd units and blueprints, warnings are errors
just test            # headless tests
just fmt             # format in place
just test-compositor # also runs the #[ignore] Wayland tests; needs a compositor
just check-units     # systemd-analyze verify on the shipped units
just check-examples  # compile every blueprint in var/widget_examples/
```

Binaries run through `just run-daemon`, `just run-panel`, `just run-wallpaper`, `just run-locker`
and `just ctl <args>`. `just nested` opens a nested niri window for a dev loop that does not disturb
the session.

`just click output=DP-2 x=1200 y=540 button=left` resolves output-relative coordinates through
`niri msg -j outputs` and injects with `ydotool`; buttons are `left`, `middle`, `right`. `ydotool`
cannot read the pointer position, so pass `restore_x`/`restore_y` in virtual-desktop coordinates
when the caller knows where to put it back.

### Never test against the live configuration

`~/.config/glimpse/config.toml` is the user's own and a binary started without `--config` both reads
and watches it.

```bash
glimpse-panel --config "$SCRATCH/config.toml"     # replaces the whole stack, drop-ins included
HOME="$SCRATCH/home" glimpse-panel                # a fake home, when drop-ins are under test
```

- `--config` is the default choice and enough for anything that is one document. It cannot exercise
  layering, because an explicit path replaces the stack rather than joining it. A test needing
  `config.d/` sets `HOME` (or `XDG_CONFIG_HOME`); one needing the `/etc/glimpse` layer builds it
  through `load_from`, which takes the system directory as an argument for exactly this reason.
- Themes redirect separately, because `theme_dir_for` resolves through `user_dir()` rather than the
  config stack: `GLIMPSE_THEMES_DIR` replaces both roots for loading and watching, `GLIMPSE_THEME`
  overrides the selected name.
- **Send the log somewhere else.** `--config` watches that file's parent directory, so redirecting
  output into it makes every line an event that reloads the configuration — with a document that
  will not parse, a closed loop running at exactly `DEBOUNCE` that looks like a retrying watcher.

### Testing against a compositor

`crates/glimpse-compositors/tests/live.rs` runs against whatever the environment names. Point it
elsewhere with `NIRI_SOCKET` or `HYPRLAND_INSTANCE_SIGNATURE` (unsetting the other): spawn
`niri -c <config>` or `Hyprland -c <config>`, diff `$XDG_RUNTIME_DIR` before and after to find the
socket, export it. Neither compositor has a headless mode, so a nested instance always opens a
window — do mutating tests there. A Unix socket path is capped near 108 bytes, so a daemon under
test gets its socket in `$XDG_RUNTIME_DIR`, never in a long scratch path.

**Urgency is set directly under niri and cannot be under Hyprland.** `niri msg action
set-window-urgent --id <id>` marks a window and `unset-window-urgent` clears it, which is what
`scripts/urgency-test.sh` wraps. Hyprland has no such dispatcher — urgency only arrives from an
`xdg_activation_v1` request it declines, so it needs a real application asking for attention. A GTK
window calling `present()` produces none: on Wayland GTK sends an activation request only when it
already holds a token.

### Previewing a widget

`just preview <path/to/blueprint.blp> [fixture]` renders one blueprint with the **real widgets** and
reloads on save. The path is resolved against the working directory — there is no search of
`var/widget_examples/`, so a bare `notifications.blp` fails. Compile errors render into the window
in red. It is a cargo example in `glimpse-widgets`, so `Builder` resolves `$PopoverShell` and
`$Hero` to the Rust types. `Esc` closes it. `--scheme dark|light` forces the scheme; a widget is not
checked until it has been seen under both.

Four things it must do, each of which fails silently otherwise:

- **Touch every widget type before building.** A Rust GType registers lazily, so a `$Hero` nothing
  has instantiated is an unknown class to `Builder`; `ensure_types` names them all. An *embedded*
  `TemplateChild<Scrubber>` needs no such call — binding it registers the type — but a blueprint
  example naming `$Scrubber` with no Rust touching it does.
- **Take `ApplicationFlags::NON_UNIQUE`.** The application ID is on the shared session bus, so a
  second preview otherwise hands off to the first, possibly on another monitor, and exits 0 with no
  window and no message.
- **Read `glimpse.css` from disk, not through `Styles::install`**, which loads it with
  `include_str!` — a preview built on that renders a compiled-in copy no edit can reach.
- **Spell a watched path the way the file monitor spells it back.** A relative argument or a `..`
  component compares unequal to the absolute path GIO reports, so every event is discarded;
  `resolve` canonicalises the blueprint and each stylesheet.

Live reload watches each file's **directory** and treats a rename onto the path as a change, because
an editor that saves via a temporary file destroys the inode a file monitor holds. The rename
arrives as `RENAMED`, whose first argument is the temporary path and whose `other_file` is the one
you asked for — match both. Events coalesce over 40ms.

The window paints a checkerboard and every child stays transparent, so whatever the widget does not
paint reads as pattern rather than a flat background it never asked for. **Both halves of that rule
must be scoped to `window.preview`**: GTK4 parents tooltips, popovers and drag icons as direct
children of the window, so a bare `window.preview > * { background-color: transparent; }` blanks
them at `USER + 2` against libadwaita's `tooltip.background`. The transparency rule names the
preview's own slot instead.

It opens floating through a `window-rule` on `^me\.aresa\.WidgetPreview` in the **user's own** niri
config — nothing here installs it. `open-on-workspace "glimpse"` is only half the rule: without a
matching `workspace "glimpse" { open-on-output "eDP-1" }` declaration the workspace is created on
whichever output is focused and previews scatter between runs. Layer-shell was tried first and
rendered nothing.

**Fixtures and stylesheets.** A fixture name defaults to the blueprint's own stem, so `calendar.blp`
shows sample events by being opened. `_shared.css` beside the example loads at `USER + 1` and
`<name>.css` at `USER + 2`, both silently absent-tolerant; the checkerboard sits at `USER + 3`.
`_shared.css` holds the shared floors — `.column`, `.drawer-page`, `.block`, `.caption`, `.slider`,
`.mute` — so demo-only rules stay out of the shipped `glimpse.css`; a name meaning something in
exactly one example stays in that example's sheet.

`var/widget_examples/` holds whole compositions (one `popover_shell_full.blp` per applet popover)
and a states board per widget, `<widget>_states.blp`, showing one instance per state under a
caption. A states board is pure Blueprint whenever the state is a property or a child; a widget fed
through a Rust setter cannot have one until something supplies that data. **An example is a
top-level object, never a `template`** — `Builder` cannot instantiate a template whose class does
not exist, so a `template` root renders as nothing.

Fixtures that run for **every** example, not a named list — gating one on a match arm is what made a
new example's drawer silently inert:

- `expanders` gives any row carrying `.expander` a handler toggling its **next sibling**, and
  complains on stderr when that sibling is not a `Gtk.Revealer`. Matching on position keeps it out
  of the blueprint. `Row`'s rule is still that it navigates rather than expands; this is the
  exception, an audio stream revealing its volume slider.
- `drawer_nav` returns immediately when the tree holds no `Gtk.Revealer`, so a popover written
  entirely in Blueprint — a `Revealer` holding a `Gtk.Stack`, rows carrying `nav__<page>` —
  navigates with no Rust. A `nav__` class with no page behind it is reported on stderr.
- `actions` logs `action: <name>` when a row carrying `action__<name>` fires, giving an example a
  way to show a command without pretending it exists. On a `$SplitRow` it connects `activated` so
  `nav__` keeps the chevron; on anything else that is a `Gtk.Button`, `clicked`. A class on
  something clickable by neither is reported.
- `pager` fills each `$Pager` from its `demo__<case>` class, since its slots come from Rust. An
  unrecognised case and a missing class are both reported.

**A widget is only declarable if it says so.** `PopoverShell` and `Hero` implement `Gtk.Buildable`,
which is what lands `[hero]`, `[footer]` and `[slot]` in the right internal box, and `Hero` exposes
`title`, `subtitle` and `icon-name` as properties so a `.blp` sets them through the same capped
setters Rust uses. `add_child` must ignore the widget's **own** template children — `init_template`
adds them through the very interface being overridden, so an unguarded override routes `hero_box`
into `content_box` and panics on an unbound `TemplateChild`. The guard is
`self.content_box.try_get().is_none()`.

**PyGObject cannot host this.** It cannot override an interface vfunc the parent already implements,
so a `do_add_child` on a Python subclass is accepted, never called. Measured. Any preview host that
needs real widgets has to be Rust.

**A blueprint cannot open a popover, and a preview must be focused to keep one open.** `active: true`
on a `Gtk.MenuButton` is applied by `Builder` before the button is realized and `GtkMenuButton` drops
it, so no popup is ever created. The class `open-on-map` on the button is what the host uses instead,
and it waits for the window to become *active* — a compositor dismisses a popup belonging to an
unfocused window with `xdg_popup.popup_done` the moment it appears, and a preview opens on its own
workspace. This is how the tray menu board is seen: `just preview var/widget_examples/tray.blp`, then
focus the window.

**`blueprint-compiler lint` has two false positives.** It reports `scrollable_parent` for any extern
`$CustomType` inside a container, and rejects a `Gtk.Adjustment` carrying anything besides `lower`,
`upper` and `value` as `adjustment_prop_order` — the order is not what it checks; adding
`step-increment` fires it regardless of position. There is no way to exclude a single rule, so
`lint-blueprints` strips ANSI colour and fails only on a line that is not `scrollable_parent`.
**Embed our own widgets declaratively** and bind them as ordinary `TemplateChild`s; set adjustment
increments from Rust, and assert them, because nothing in the template guards them any more.

## Translations

One gettext domain, `glimpse`, for all binaries. `glimpse-utils` owns it: `init_translations()`
binds it, and the panel, notification popup, lock screen and wallpaper call that once in `run`. The
daemon, `glimpsectl` and `glimpse-sunset` do not — their output is a journal and a terminal.

```bash
just extract-strings     # rewrite po/glimpse.pot from the tree
just update-po           # merge the .pot into every catalog named by po/LINGUAS
just build-translations  # compile po/*.po into target/locale/<lang>/LC_MESSAGES/glimpse.mo
just check-strings       # part of `just verify`
GLIMPSE_LOCALE_DIR=$PWD/target/locale LANGUAGE=ru just preview <blueprint.blp>
```

- **`glimpse-sunset` and `glimpse-weather` call `init_locale()` instead, and must keep doing so.**
  That is the `setlocale(LC_ALL, "")` half without the catalog. Without it the process locale is
  `C`, `nl_langinfo(LC_MEASUREMENT)` answers metric for everyone, and `[regional] units = "locale"`
  is silently wrong rather than absent. It is not `init_translations` and must not become it.
- **`[regional] language` sets `LANGUAGE` and moves messages only.** `LC_TIME` and `LC_MEASUREMENT`
  keep answering for themselves, so a Russian interface in Chicago still gets a twelve-hour clock
  and Fahrenheit. Russian labels beside English weekday names look like a bug and are not. An
  explicit `LANGUAGE` in the environment wins over the document, matching `GLIMPSE_THEME`.
- **The config load runs before `init_translations`** in all three UI binaries, because the language
  comes out of the document: `init_app_tracing` → `glimpse_config::load` → `init_translations` →
  `register_resources`. Reordering breaks the warnings, the language, or both.
- **A language change cannot be applied to a running process.** A GTK template resolves
  `translatable="yes"` per **instance**, so two widgets built either side of a `LANGUAGE` change
  come out in different languages. `ConfigChanged` logs at `info` and waits for a restart.
- **Mark a string where it is written.** In Blueprint, `_("Text")`; in Rust, `gettext("Text")`, or
  `ngettext(singular, plural, count)` when a number decides the wording. Interpolate with named
  `{placeholders}` and `.replace(…)`, never `format!` into the msgid — a positional `%s` cannot be
  reordered by a translator.
- **Double quotes, always.** Blueprint compiles `_('Text')` happily; xgettext's C scanner skips it.
  `scripts/i18n-coverage.py` fails the build on it.
- **No translatable text in a raw or multi-line Rust string.** `r#"…"#` extracts by accident and the
  C scanner loses its place inside both, costing the rest of that file.
- **A new file's strings are extracted as soon as the file exists.** `scripts/i18n-extract.sh`
  builds its list with `rg --files`, not `git ls-files`, so an untracked `.rs` or `.blp` is picked up
  without staging it. `grep -c '^msgid ' po/glimpse.pot` is still the number that tells the truth
  when the count looks wrong.
- **`var/` is not extracted.** Its examples carry `_()` markers so they read like the real thing,
  but they never ship.
- **A new language is three edits:** `po/LINGUAS`, a new `po/<lang>.po`, and one asset line in
  *each* of the two lists in `crates/glimpse-package/Cargo.toml`.

## Work rules

- Work on one feature at a time, and only start the next after the current one passes end-to-end
  verification. Don't "also refactor" feature B while implementing feature A.
- Spawn desktop windows on the `glimpse` niri workspace; do not steal focus.
- **Do not commit or push without being asked.**
- **Never hand work back without running the pass in Finishing.**

## Critical constraints

- **Never add `panic = "abort"` to any profile.** Per-service panic isolation depends on unwinding;
  abort turns one bad handler into a dead daemon and takes tray and notifications down with it.
- **No service crate has a Wayland dependency.** Wayland objects belong to the owning UI or
  compositor crate — `glimpse-services` reaches a compositor only through `trait Gamma` and
  `glimpse-compositors` — while pointer injection belongs in a script.
- **`_old/` and `var/glimpse2` are reference only.** Never edit, build, or copy code out of them.
- **Never sandbox `glimpse-lock.service`.** `NoNewPrivileges=`, `PrivateUsers=`,
  `RestrictSUIDSGID=` and anything implying them strip setuid from `unix_chkpwd`. PAM then returns
  `AUTHINFO_UNAVAIL` and the correct password is rejected, which looks like a wrong password and is
  expensive to diagnose.
- **No unit relationship may stop `glimpse-lock.service` while it holds the lock.** A stopped locker
  is a locked session with nothing to authenticate against. `PartOf=` on anything but
  `graphical-session.target`, `BindsTo=`, or someone else's `Conflicts=` all reach that state;
  `Wants=`/`WantedBy=` cannot.
- **No `unwrap()`, `expect()`, or blocking calls in a service handler.** A panic stops that service
  and cascades `degraded` to its dependents; blocking `std::fs`, `Command::output()`, or a
  `std::sync::Mutex` held across `.await` freezes every other piece of state the service owns,
  because handlers run serially on `&mut self`.
- **Never shell out to `systemctl`, `loginctl`, `nmcli`, `bluetoothctl`, or `niri msg`.** Use D-Bus
  or the compositor's IPC socket. Subprocesses cannot be mocked, break under sandboxing, and parse
  output that is not a stable interface.
- **A glimpse process writes runtime state under `$XDG_RUNTIME_DIR/glimpse/` and nothing else.**
  Never `$XDG_CONFIG_HOME`, never the user's home, never `/tmp`.
- **Treat text from other applications as hostile.** Tray titles, notification summaries and bodies,
  MPRIS metadata and SSIDs are attacker-controlled and unbounded. Cap length, ellipsize, and
  sanitize markup before any of it reaches a label.
- **Never hand-roll what a library already does.** Search the standard library, then
  `[workspace.dependencies]`, then crates.io, and stop at the first hit. Before adding a `fn` that
  parses, formats, resolves, encodes or retries anything, read the root `Cargo.toml` — the answer is
  often already declared and unused.
- **Check the lockfile before proposing a crate.** A direct dependency's own dependencies are in
  `Cargo.lock` and already built, so promoting one adds no supply-chain surface and no build time.
  `grep '^name = "x"' Cargo.lock` turns a proposal into a one-line addition.
- **Propose a new dependency, never add one silently**, and name the exact current version, looked
  up rather than recalled — a remembered version is usually a year stale and resolves against an API
  that has moved. Wait for confirmation before it lands in `[workspace.dependencies]`.
- **Use `just`, never raw `cargo`.**

## Known state

Facts measured about code that would otherwise invite rework, with what was counted and when.

**`glimpse-config/src/watch.rs`, August 2026.** 347 production lines against 391 of tests. `Watch`
and `Arm` — arming inotify, falling back onto an ancestor when `config.d/` does not exist, re-arming
when a directory is replaced under it — are **44%** of the production half: four directories are
watched and two of them usually do not exist. Every branch has a test and it has not been the source
of a bug, so its size is not by itself a reason to rewrite it. Both defects found here were in the
simple 18% that decides whether to reload, or in the harness testing it. Bead `glimpse-aqi5` records
the one limitation the design knowingly accepts.

What is genuinely dead, and is the cut to make in whichever change next touches the file: `Changed`
carries a `Vec<PathBuf>` that nothing reads. Both consumers of `Update` — `watch_config` and
`theme.rs`'s `watch_theme` — collapse `Changed` and `Rearmed` into one arm, so the only distinction
the tree draws is "something happened" against "the watch is dead". Those paths are still collected
in `forward`, carried through the channel and filtered in `Watch::next` to build a value nobody
reads.

**Translations, September 2026.** 45 msgids: 17 from 8 marked blueprints, 27 from 5 Rust files, one
("Play") in both. Measured end to end — under `LANGUAGE=ru` a `$Transport` built from its gresource
template returns Russian tooltips. Scanning the whole tree costs **0.14s** and produces a
byte-identical .pot, so extraction takes every `.blp` and `.rs` with no marker filter: a filter that
was ever wrong would hide a file from the extractor and the coverage check at once. xgettext has no
Rust scanner and the C fallback loses the rest of a line to `&'static str`; `scripts/i18n-scrub.py`
and `scripts/i18n-coverage.py` carry the full account, which is why those two warnings are filtered
rather than chased. `just fmt` shifts line numbers and so makes `po/glimpse.pot` stale —
`just check-strings` reports it and names the recipe; that is normal. There is no `dpkg-deb` or
`rpm` on Arch, so `bsdtar` is how built packages are confirmed to carry the `.mo`.

**StatusNotifierItem in the wild, September 2026.** Four items on a live session bus — vicinae,
Slack (Electron), walz (libayatana-appindicator), Telegram (Qt) — introspected and probed.

An item implements a *subset* of `org.kde.StatusNotifierItem`, and a typed zbus getter is the wrong
way to read one. zbus 5.19.0's `get_property` (`proxy/mod.rs:783`) reads the cache, misses, then
issues a real `Get` that **errors** for a property the peer does not implement: Slack's `IconName`
fails while its `IconPixmap` answers, and walz is the exact mirror — `IconName` holding an absolute
path, no `IconPixmap` at all. Slack's `Introspect` returns an empty `<node></node>` while every
property still answers, so introspection is not a discovery mechanism either. `GetAll` works on
both, returning only what each implements. **Read an item as one explicit `GetAll` through
`zbus::fdo::PropertiesProxy` and decode the map with defaults**, the same shape the dbusmenu layout
already uses; build the item proxy `CacheProperties::No` so the cache does not fetch it twice.
`Proxy::cached_property` alone is not the answer — it returns `None` for a cache merely not yet
populated, and `get_property_cache` is `pub(crate)`, so there is no public readiness to await.

Ayatana is not a second protocol and needs no branch: same interface name, same watcher, extra
optional members. The deltas are the object path (`/org/ayatana/NotificationItem/<id>`, so split a
registration string at the **first** `/` and never assume `/StatusNotifierItem`), `XAyatanaLabel` /
`XAyatanaLabelGuide` / `XAyatanaNewLabel(ss)`, `XAyatanaOrderingIndex`, `XAyatanaSecondaryActivate(u)`
taking a timestamp where `SecondaryActivate` takes `(ii)`, the `*AccessibleDesc` properties, and
`NewIconThemePath(s)`. One of the four items speaks it, and its `XAyatanaLabel` is empty — so it is
the only candidate source for a chip badge and nothing feeds it. Render it as `label` when non-empty
and leave `badge` unfed rather than inventing a split.

**What the epic itself measured.** A tray item is decoded from one `GetAll` and every field has a
default, which is what makes an application's partial implementation ordinary rather than an error
path. `Registry` holds the watcher's rules as a plain struct so they are tested without a bus. The
name freeing is the trigger for a re-claim and there is no timer anywhere in the tray. A fresh owner
must announce *before* it sweeps, or a client that re-registers on the announcement and is also swept
appears twice — the canonical key is what collapses them. `just click` proved unusable for verifying
any of the UI here (see `glimpse-cd67`); the tray menu was verified by opening it from the preview
host instead, which is what `glimpse-fxc0.1` turned out to be about.

**BlueZ on this machine, September 2026.** One adapter, `PowerState` present on a stock non-
experimental 5.87, measured against a WH-1000XM4, a Keychron K3 and a 20-second scan.

- **No battery is the common case for audio devices.** A *connected* WH-1000XM4 carried no
  `Battery1` interface at all — BlueZ derives headset battery from the Apple HFP extension, which
  sits behind `Experimental = true`. `Battery1` is an interface-presence question, arriving through
  `InterfacesAdded` on a device path that already exists and leaving the same way, so both must be
  handled on an existing device rather than only at creation.
- **A discovery session belongs to the D-Bus connection that started it.** `busctl StartDiscovery`
  returns success and starts nothing, because busctl exits; `Discovering` is never an
  acknowledgement of our own command, only the adapter's state. Through a client holding its
  connection open, 17 devices arrived in 20 seconds.
- **`ObjectManager` is at `/`, not at `/org/bluez`.** `GetManagedObjects` on the tree root answers
  `org.freedesktop.DBus.Error.UnknownMethod`, and `InterfacesAdded`/`InterfacesRemoved` are emitted
  from `/` as well — so a `path_namespace` of `/org/bluez` matches neither. It matches
  `PropertiesChanged` and `Disconnected`, which do come from below it. Every headless test passed
  with the wrong path; only a live run found it.
- **`br-connection-key-missing` means the remote forgot the bond**, not that it refused — seen on a
  paired phone that had been reset. It is `Failure::BondBroken`, and re-pairing is the fix.
- **`Introspect` over-reports** — `Adapter1.ExperimentalFeatures` is advertised, omitted from
  `GetAll`, and refused by `Get`. This is the opposite direction from Slack's empty `<node></node>`
  in the tray research, and the same conclusion: one `GetAll`, decoded with defaults.
- **`Device1` splits systematically between bonded and discovered**, not per vendor. `RSSI`,
  `TxPower` and `ManufacturerData` are absent on a bonded device and present on a discovered one;
  `Name`, `Class`, `Icon` and `Modalias` are the reverse. `Alias` is always answered, synthesized as
  the dashed MAC, which is why the display name is `Alias` with no fallback chain.
- **`PowerState` is documented `[experimental]` and answers anyway**, and its `off-blocked` gives
  rfkill state with no `/dev/rfkill` dependency. Decode it with a default so an older BlueZ falls
  back to `Powered` and simply never enters the blocked state.
- **Connecting adds `dev_XX/fd0` and `sep1`…`sep6` as child objects.** Match a device path by shape —
  exactly one `dev_*` segment below the adapter — or a prefix match invents seven phantom devices per
  connection. `fd0` is the `MediaTransport1`: its `Codec` is `0xFF` for LDAC and the first six bytes
  of `Configuration` are the company/codec tuple (`2d 01 00 00 aa 00` → Sony/LDAC). The codec *name*
  is obtainable; the bitrate is a PipeWire quality tier BlueZ never sees. `MediaEndpoint1.Vendor` is
  documented but useless here — six endpoints exist per connection and nothing links one to the
  transport in use, so the transport's own `Configuration` is the only reliable route.
- **`Device1.Disconnected(reason, message)` exists in 5.87** and gives the drop reason for free.
  `Local` and `Suspend` are not failures; the `message` is bluetoothd's own English and is logged,
  never shown.
- **The agent is per-D-Bus-connection**, so two panels each register their own and never collide.
  `RequestDefaultAgent` is a system-wide singleton and is never needed.
- **`busctl --system monitor` needs root** (`BecomeMonitor` → access denied). `gdbus monitor --system
  --dest org.bluez` uses match rules and works unprivileged.
- **A dialog presented on a never-mapped window is queued, not shown.** `adw::AlertDialog::present()`
  against the panel's hidden `adw::ApplicationWindow` leaves it `mapped=false` with no error;
  showing the host maps it at once and niri lists a real toplevel. Any global dialog on that host
  must show the host while one is up.

**A `Gtk.PopoverMenu` renders here perfectly well; three things make it look as if it does not.**
A popover's anchor rectangle is its *parent's allocation*, so a parent filling the window anchors the
popup to the window's own edge — with a bottom gravity on a full-height window the compositor
squeezes it to a few pixels and GTK tears it down. A compositor also dismisses a popup belonging to
an **unfocused** window with `xdg_popup.popup_done` the moment it appears, and a preview opens on its
own workspace, which is usually not the focused one. And `active: true` on a `Gtk.MenuButton` in a
`.blp` does nothing at all: `Builder` applies it before the button is realized and `GtkMenuButton`
drops it, so no `xdg_positioner` ever reaches the compositor. `WAYLAND_DEBUG=1` and the
`set_anchor_rect` / `configure` pair tell the three apart in one run.

**`Status` is two properties on two interfaces.** `org.kde.StatusNotifierItem.Status` is
`Active`/`Passive`/`NeedsAttention` — and `Passive` is a placement instruction, the host tucks the
item away, not a tint. `com.canonical.dbusmenu.Status` is `normal`/`notice`, on the menu object.
Both read their quiet value on every live item, so nothing on screen separates them.

## Finishing

Finishing is a pass over the work, not the moment the last edit compiles. Run it every time, before
saying anything is done. **Findings are work, not notes** — fix them and run the pass again. It ends
when a full pass turns up nothing, not when the list gets short.

1. **Formatter and linter clean.** `just fmt`, then `just lint`, and `just verify` when code
   changed. Zero errors and zero warnings — clippy runs with `-D warnings`. Silencing one with
   `#[allow(...)]` rather than fixing it needs a reason worth saying out loud.
2. **Delete what nothing calls.** Dead functions, unused constants, a type kept "for later", a
   wrapper whose body is a single call, a trait with one implementation.
3. **Cut the ceremony.** A custom error type carrying no information a message would not, a builder
   for two fields, a helper called once, a test asserting that the standard library works.
4. **Check the docs the change invalidated**, the crate `README.md` first — a stale README is worse
   than none, because it is believed.
5. **Read it as a stranger would.** If any part would need an apology, that part is the finding.

## Keep the documentation current

A stale document produces confidently wrong work, which costs more than the document saved.

- **Update this file whenever you learn something that would change how the next agent works** — a
  non-obvious gotcha, a command that turns out to be wrong, a constraint that stopped being true.
- **Update the crate's `README.md` in the same change that alters what the crate does.**
- Remove instructions that stop being true rather than adding a caveat beside them. Two rules on the
  same topic produce worse behaviour than one.

**A crate `README.md` is capped at 300 lines, and states rules, not history.** Over the cap, cut
until it fits. Write the rule that is true now, in the present tense, and delete the one it
replaces. None of this belongs in a README: what the code used to do; bead numbers as evidence; what
you tested or chose not to test; a story where a clause would do. Keep what the thing is, the rule,
and the one consequence that explains why the rule exists. When a change makes a README longer, ask
which existing paragraph it replaces — a rewrite that only ever appends is a symptom. A genuinely
measured fact that would otherwise invite rework goes in **Known state** above, which says what was
counted and when precisely because a README should not.

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
