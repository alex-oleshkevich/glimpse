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
├── var/          scratch, not installed; `var/glimpse2` holds third-party design drafts
└── _old/         the previous implementation, kept for reference only
```

| Crate                   | Role                                                                              |
| ----------------------- | --------------------------------------------------------------------------------- |
| `glimpse-dbus`          | D-Bus proxies and the shared bus connections                                      |
| `glimpse-config`        | layered TOML load, drop-ins, merge, validate, watch                               |
| `glimpse-compositors`   | niri and Hyprland IPC: snapshot, events, keyboard/workspace/window/output control |
| `glimpse-services`      | service framework and every service implementation                                |
| `glimpse-widgets`       | GObject subclasses, Blueprint templates, shared CSS, compositor blur              |
| `glimpse-utils`         | shared CLI arg structs, tracing/log setup, gettext binding and text cleaning      |
| `glimpse-panel`         | panel and applets                                                                 |
| `glimpse-notifications` | notification owner, typed D-Bus provider and transient popup layer surface        |
| `glimpse-weather`       | weather provider                                                                  |
| `glimpse-wallpaper`     | background layer surface, decode cache, transitions                               |
| `glimpse-lock`          | `ext-session-lock-v1` surfaces, PAM                                               |
| `glimpse-sunset`        | night-light service                                                               |
| `glimpse-picker`        | color picker CLI: screencopy, lens overlay, prints the picked color               |
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
  `[wallpaper]` and `[lock]`. A binary reads only the tables it owns — except that the lock
  inherits `[wallpaper]`'s `image` and `image-dark` while `[lock.background]` names neither, because
  a lock screen that differs from the desktop by default reads as a bug. Stylesheets stay separate:
  `panel.css`, `lock.css`, and one `dark.css` per theme that every surface loads while the effective
  scheme is dark. `[appearance] theme-variant` is a CSS class on every window, not a file.

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
- The one sanctioned exception: **a settled pairing trusts and connects**, because BlueZ leaves a
  freshly bonded device bonded and not connected, and a user who pressed *Pair* meant *use this*.
  Nothing else may grow a policy on top of a backend without the same explicit note here.
- The second, for the same reason: **eject unmounts first**. UDisks2's `Drive.Eject` refuses with
  `DeviceBusy` while any filesystem on the drive is mounted and offers no option to unmount, so a
  pass-through fails in the one case that matters — the drive the user has just finished using, which
  this shell mounted itself. A failed unmount is reported as itself and the eject never runs;
  `NotMounted` and `AlreadyUnmounting` are races, not failures, and are stepped over. Only the
  drive's own volumes are touched, never another drive's.
- The third, for the same reason: **an incoming service from a bonded device is authorized**.
  `Agent1.AuthorizeService` is BlueZ delegating rather than deciding, and it asks only about an
  *untrusted* device — so a blanket refusal strands every pairing made before glimpse, with no UI
  anywhere to explain it. The bond is the whole test; nothing else is consulted.
- Commands are thin pass-throughs to the backend.
- A handler that can block moves its `Responder` into `ctx.spawn`. Handlers run serially, so one
  slow D-Bus call otherwise freezes the whole service.

**UI**

- An applet renders topics and sends commands. It never opens a D-Bus connection, never reaches a
  backend directly, and holds no state that outlives its own widget.
- UI state never waits on a round trip. Update the widget optimistically and let the topic event
  reconcile it.
- **A failed command is reported by a notification and never by a banner in the popover.** A popover
  is open for seconds and the failure outlives it; `spawn_reported` already posts one, so a `$Notice`
  beside the row says the same thing twice and leaves a stale sentence behind on the next open. The
  one banner that is allowed is a condition notifications themselves cannot carry — the notification
  provider being down.
- A widget moves to `glimpse-widgets` as soon as a second binary needs it.

## Verification

`just` is the only entry point; run it with no arguments to list recipes. A recipe that is missing
or wrong gets fixed in the `justfile` — never worked around with a raw cargo invocation.

**A recipe body is one command; anything that needs a shebang goes in `scripts/`.** No `#!` block
lives in the `justfile`. A loop, a branch or a second line is a `scripts/<recipe>.sh` the recipe
calls, and a justfile variable reaches it through the environment — `GLIMPSE_BINARIES`,
`GLIMPSE_LANGUAGES`, `GLIMPSE_ELEVATE` — with arguments passed as `"$@"`, never interpolated.

```bash
just verify          # fmt-check + check + lint + test — what CI runs
just check           # type-check, fast
just lint            # rust, systemd units and blueprints, warnings are errors
just test            # headless tests
just fmt             # format in place
just fmt-blueprints  # format blueprints; pass paths, or every one by default
just test-compositor # also runs the #[ignore] Wayland tests; needs a compositor
just check-units     # systemd-analyze verify on the shipped units
just check-examples  # compile every blueprint in var/widget_examples/
just gen-config-commented  # regenerate the seed installed into ~/.config/glimpse
just net-guard arm   # restore connectivity automatically if a network test strands the machine
```

Binaries run through `just run-daemon`, `just run-panel`, `just run-wallpaper`, `just run-locker`
and `just ctl <args>`. `just nested` opens a nested niri window for a dev loop that does not disturb
the session.

**A blueprint error naming a `.blp` that does not exist is a stale build script, not a broken tree.**
Switching branches can leave `target/` holding a compiled `build.rs` from the other branch whose
fingerprint cargo does not invalidate, so it re-runs the *old* list and fails on a blueprint the
current `build.rs` never mentions — the give-away is that the missing name is absent from
`crates/glimpse-widgets/build.rs` and sits one past its last entry in the `rerun-if-changed` output.
`touch crates/glimpse-widgets/build.rs` forces the recompile. Read the failing name against
`build.rs` before believing anything is actually missing.

`just click output=DP-2 x=1200 y=540 button=left` resolves output-relative coordinates through
`niri msg -j outputs` and injects with `ydotool`; buttons are `left`, `middle`, `right`. `ydotool`
cannot read the pointer position, so pass `restore_x`/`restore_y` in virtual-desktop coordinates
when the caller knows where to put it back.

### Never test against the live configuration

**`direnv` exports `GLIMPSE_CONFIG_PATH` in this repo, and it beats a scratch `HOME`.** `.envrc`
sets `GLIMPSE_CONFIG_PATH=var/config/config.toml` along with `GLIMPSE_PANEL_APP_ID` and
`GLIMPSE_WALLPAPER_APP_ID`, so a binary launched from inside the working tree silently loads the
dev document even when `HOME` points at a scratch directory. Override it explicitly, and read the
`load config path=` line in the log before believing anything on screen.

**Give every test panel its own `GLIMPSE_PANEL_APP_ID`, and never reuse one.** The application ID
is a unique name on the session bus, so a second panel claiming an ID the previous run has not
finished releasing hands off to that instance instead: it logs `load config path=`, never reaches
`initializing app`, maps no window, and sits there looking like a hang. Measured — the same trap the
preview host answers with `ApplicationFlags::NON_UNIQUE`. A fresh ID per run costs nothing.

**`pkill -x glimpse-panel` kills the SESSION panel.** `-x` is the right answer to `-f` matching its
own shell, but the session binary is also called `glimpse-panel`, so an exact-name kill takes the
user's bar down with the test's. Kill by the pid you started, and if the session panel does go,
`systemctl --user start glimpse-panel.service` brings it back.

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
shows sample events by being opened. The theme's `dark.css` loads at `USER + 1`, and only while the
scheme is dark — any sheet named `dark.css` is gated that way. `_shared.css` beside the example loads
at `USER + 2` and `<name>.css` at `USER + 3`, all silently absent-tolerant; the checkerboard sits at
`USER + 4`.
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
- `busy` spins anything carrying the `busy` class. A `$SplitRow` spins through its inner `$Row`,
  because `SplitRow` exposes no `busy` property of its own and its `row` is a template child a
  blueprint cannot reach; a `$Row` can equally say `busy: true` in the blueprint and needs no class.
  Anything else carrying the class is reported, since it has no spinner to turn on.
- `indicators` configures each `$Indicator` from `icon__<name>`, `overlay__<name>`, `label__<text>`,
  `severity__<info|warning|error>`, `state__attention` and `state__notice` — `Indicator` has
  **no GObject properties at all**, so a states board cannot otherwise set one from Blueprint. An
  indicator carrying **none** of those classes is left completely alone: `TrayStrip` builds its own
  chips, and a fixture that wrote to every `Indicator` it found would wipe them. The two flags are
  namespaced because bare `attention` and `notice` are already styling hooks elsewhere — on a
  `Gtk.Image` in `workspaces.blp` and on the `Notice` template — and either landing on an
  `$Indicator` would otherwise be read as state.

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
binds it, and the panel, notification popup, lock screen, wallpaper and `glimpse-ruler` call that
once in `run`. The daemon, `glimpsectl` and `glimpse-sunset` do not — their output is a journal and
a terminal. `glimpse-picker` does not either — its only on-screen text is a raw color value, never a
phrase.

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
- **Never sandbox `glimpse-lock.service` — no systemd sandboxing option of any kind.** All 14
  measured in `var/lock/research.md` break PAM through one of two mechanisms: namespace options
  (`PrivateTmp=`, `ProtectSystem=`, `ProtectKernelTunables=`, …) put a user service in a user
  namespace where root is unmapped and `unix_chkpwd`'s setuid bit is not honoured, and
  seccomp-family options (`SystemCallFilter=`, `LockPersonality=`, `RestrictSUIDSGID=`, …) imply
  `NoNewPrivileges`. PAM then returns `AUTHINFO_UNAVAIL` and the correct password is rejected, which
  looks like a wrong password and is expensive to diagnose. The daemon probes itself at start and
  refuses to lock when either mechanism is present, except when `LockedHint` is already true at
  start, where it locks behind a prompt that sends the user to a text console.
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

**`first-day` has no `locale` variant, and that is the convention working, September 2026.** GTK's
translated `calendar:week_start:0` came back as the untranslated msgid — meaning Sunday — under an
`LC_TIME` whose answer is Monday, and `_NL_TIME_FIRST_WEEKDAY` has never been measured here. A
setting only gets `locale` when the system can actually be asked, so adding the variant needs that
measurement first.

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
- **A forgotten device that is merely switched on is connectable, not pairable.** Removing the bond
  leaves the remote holding its own, so powering it on puts it back under *Nearby* offering *Pair
  this device*, and `Pair()` answers `org.bluez.Error.AuthenticationRejected` — the remote refuses
  because it is not in pairing mode. Measured on a WH-1000XM4. Nothing in glimpse can fix it; the
  wording is all that is ours, and it names the button on the device.
- **`StopDiscovery` answers `Failed: No discovery started` whenever the deadline got there first.**
  The scan timeout stops discovery and clears the service's own bookkeeping, and the popover's
  `unmap` then stops unconditionally, by design. The second stop is routine, so `classify` reads it
  as done rather than as a refusal.

**Service hardening blinds the camera detector, September 2026.** `glimpse-panel.service` carried
`ProtectKernelTunables`, `ProtectKernelModules` and `ProtectControlGroups`. Each implies
`MountAPIVFS=yes`, which remounts `/proc` private — and inside that mount
`readlink /proc/<other-pid>/fd/N` returns the **empty string** while the fd directory still lists its
entries. The `/proc` scan therefore finds no holder, reports no camera, and logs nothing: `readlink`
failing lands in `.unwrap_or(false)`. Measured against `ffmpeg` holding `/dev/video0`, with
`uvcvideo`'s refcount at 1 throughout: a shell finds the holder, `systemd-run --user -p
MountAPIVFS=yes` finds none, and `ProtectProc=default`, `PrivateMounts=no` and `ProcSubset=all` each
fail to rescue it. The three options are dropped from the panel unit; the other units keep them,
because only the panel hosts the privacy service. For a **user** service they buy little anyway — an
unprivileged process has no `CAP_SYS_MODULE` and cannot write `/proc/sys` or the cgroup tree.

**`RestrictSUIDSGID=` breaks every JPEG decode, September 2026.** gdk-pixbuf 2.44 hands JPEG to
glycin, which starts a `bwrap --unshare-all` sandbox per image. Under `RestrictSUIDSGID=true`, bwrap
exits with status 1, so `Pixbuf::file_info` and `from_file_at_scale` both fail. Bisected with
`systemd-run --user -p <option>` over each option in `glimpse-wallpaper.service`: it was the only one
that broke decoding. The wallpaper then showed its solid color, and before the `rendered` guard in
`Surface::decoded` it re-spawned the decode forever: 1h28m CPU in 43 min wall, with three bwrap
sandboxes per round, and niri sluggish throughout. The option is dropped from the wallpaper, panel and
notification units — every unit whose process decodes an image.

**One DDC/CI transaction on the built-in panel's bus freezes it, September 2026.** The eDP-1 OLED
(Samsung ATNA60CL10, amdgpu) holds its last frame and ignores every flip until a modeset
(`niri msg output eDP-1 off` then `on`, or a reconnect). niri keeps presenting at 120 fps
throughout — a frame counter on eDP proved it — so nothing above the kernel looks wrong. Bisected by
restarting one service at a time: only `glimpse-panel` froze it, because `DdcBacklight` probed every
connected connector, eDP included. `ddcutil --bus 4 getvcp 10` (eDP's legacy `ddc` bus) alone
reproduces it with no glimpse process running; the AUX bus `i2c-13` did not. It also fires with no
restart, since brightness re-enumerates on display hotplug. For hours it read as a wallpaper bug and
then as amdgpu PSR, because a restart is when the screen gets looked at. Built-in connectors are now
never probed.

**A long-running panel that has "stopped" reporting something is the first thing to disprove.** Both
sides of this were `/usr/bin/glimpse-panel` with an identical environment and config; only one ran
under systemd, and that was the whole difference. Compare a probe started from the shell against the
session panel *at the same moment, with the same stimulus held* before believing the code is at
fault — and give the probe a fresh `GLIMPSE_PANEL_APP_ID`.

**Printing has two test harnesses and they are not interchangeable, September 2026.**
`scripts/printing-mock-cups.py` **replaces** cups — it serves IPP itself and the panel is pointed at
it with `[printing] server-url`, so no part of the real daemon runs. `scripts/printing-network-
printer.sh` goes the other way: `ippeveprinter` (shipped with cups, no root, high port) serves a real
IPP Everywhere printer, avahi advertises it, cups-browsed creates a temporary queue, and the applet
reads it through the ordinary local cups server. Use the mock for widget states, the other for
discovery and job control. Measured end to end here: cups, cups-browsed and avahi-daemon are all
active, and a queue appears within seconds.

- **A discovered queue answers `lpstat -e` but not `lpstat -p` until something instantiates it**, so
  a readiness check written on `-p` waits forever on a printer that is already there.
- **Jobs need `lp -H hold` to stay in the queue at all.** `ippeveprinter` completes a job almost
  instantly, so without the hold there is nothing to render; `--slow N` sets a print command that
  sleeps, which is the only way to watch `processing` and a page count.
- **`cupsdisable -r` is the only route to `printer-state = stopped`**, and so the only way to reach
  `render::attention`. It needs the queue instantiated by one job first — before that cups answers
  `client-error-not-found` on a discovered queue. It sets `printer-state-reasons = paused` and puts
  the text in `printer-state-message`, which **the applet does not read**.
- **A supply reason cannot reach this applet at all.** `ippeveprinter`'s /supplies and /media forms
  do produce real `toner-low-report`, `toner-empty-report`, `media-low-report` and
  `media-empty-report` keywords — read back off the emulator's own port to confirm it. None of them
  reach cups: a discovered queue reports `printer-state-reasons = none` idle *and* mid-job, and a
  permanent `lpadmin` queue on the same device does the same. `fetch_printers` asks the cups server
  with `CUPS-Get-Printers`, never the device, so low ink and out-of-paper are unreachable without
  the applet reading the printer directly. The harness does not pretend to offer them.
- **Avahi advertises on every interface**, docker bridges and veths included, so one printer appears
  half a dozen times in `avahi-browse`. That is the machine, not a bug in the advertisement.
- **A backgrounded emulator dies with the shell that started it.** `scripts/printing-network-
  printer.sh run` owns it in the foreground and tears it down on `^C`, which is why that is the mode
  to use by hand; `up` is for scripted use and needs the caller to keep the shell alive.

**Screencopy for the color picker, September 2026.** niri 26.04 offers `zwlr_screencopy_manager_v1`
and no `ext_image_copy_capture`. Captured on this machine: DP-2 3840x2160 and eDP-1 2880x1800 (scale
1.25), both `Xrgb8888`, `y_invert` false, transform `Normal`, two outputs in about half a second. A
nested niri's winit output reports `Flipped180`, and the frame `Frame::upright` builds from it
matched the live window — `magick compare -metric AE` of the two screenshots gave 0.07 — so the transform path is exercised by `just nested`; the
quarter turns were never captured from a rotated output — the nested winit output ignores
`niri msg output winit transform`, a nested Hyprland cannot map its window under niri
(`must ack the initial configure before attaching buffer`), and no output here is rotated. Instead
`every_transform_matches_the_mapping_grim_uses` checks all eight transforms, with and without
`y_invert`, against a transcription of grim's own buffer-to-logical matrix (`render.c`: centre,
`y_invert` scale, rotate by the output's angle, flip x); reverting `_90` to the first version fails
it. A `Layer::Overlay` surface with
`KeyboardMode::Exclusive` maps and lists under `niri msg layers` as exclusive.

**Data-control and the clipboard, September 2026.** niri 26.04 implements **both**
`ext_data_control_manager_v1` and `zwlr_data_control_manager_v1` (read out of the binary; smithay
compiles both selection handlers). `ext` is bound first as the standardised successor. The protocol
carries **no client identity at all** — `ext_data_control_offer_v1` has one event, `offer(mime_type)`
— so a row can never name the application a selection came from, and guessing it from the focused
window is wrong whenever a script or a clipboard tool wrote it.

- **Reading exactly the byte cap truncates silently.** `reader.take(CAP)` returns a full buffer for
  any oversize selection, which then *passes* a cap comparison downstream and is stored as a
  fragment that pastes back as corrupt content. Read `CAP + 1` and discard above `CAP`. Found by a
  live `wl-copy` of 3 MB, not by any test — it needs a real peer writing more than the cap.
- **`x-kde-passwordManagerHint` must be classified off the mime list before any content is read**,
  so a secret never enters the process. `wl-copy --sensitive` exercises it end to end;
  `wl-clipboard` is installed here, which makes this the most live-testable applet in the tree.
- **`gdk::Texture::from_bytes` decodes at full size.** A byte cap says nothing about a pixel count
  and a small file can decode to an enormous bitmap, so clipboard images go through
  `glimpse_widgets::thumbnail`, which asks the loader for its dimensions and scales during decode.
- **`DefaultHasher` has fixed keys** and produces identical output in every process, so a content
  fingerprint built on it is forgeable by the application that chose the content. `RandomState` per
  instance, and compare the content on an id match.
- **`pgrep -f <pattern>` matches the shell running it**, so `pkill -f glimpse-panel` kills its own
  command and the tool reports exit 144. Use `pkill -x`.
- **`data_offer` precedes `selection` OR `primary_selection`**, so a mouse highlight delivers an
  offer too. Ignoring the `primary_selection` arm leaks a map entry and an undestroyed compositor
  resource per highlight — thousands a day. Destroy it even when the value is unwanted. Measured:
  65 highlights, 0 KB RSS growth once handled.
- **A data-control connection must follow demand.** Constructing the backend unconditionally means
  `enabled = false` still opens a socket and reads every copy into the process, which is the exact
  promise that setting exists to make. Gate on the first `events()` subscriber and release the
  connection when the last one goes; that also makes a re-enable work, which a `subscriptions()`
  key leaving and returning otherwise cannot.
- **Dropping a `spawn_blocking` `JoinHandle` does not cancel the task.** A retry loop that re-spawns
  instead of re-awaiting leaks one pool thread per attempt; relm4 caps the pool at 512, after which
  every other `spawn_blocking` in the panel stalls. Carry the handle, as `glimpse-idle` does.
- **Keep every blocking setup step inside the timeout.** A `roundtrip` left on an async worker is
  not covered by a timeout wrapping only the `spawn_blocking` before it, and parks a share of the
  runtime for as long as the compositor stays silent.
- **`ydotoold` is not running here**, so `just click` cannot drive the panel at all — clicking a
  popover row stays on the manual list.

**Compositor blur, September 2026.** niri 26.04 implements `ext-background-effect-v1` and advertises
capability `0x1` (blur); `var/blur-probe` blurred a panel bar, a popover body inside a full-output
catcher, and a notification stack, each through its own region; `glimpse_widgets::blur` is the
shipped version, and its README carries the design.

- **A niri `layer-rule` with `background-effect { blur true }` blurs the whole surface**, so it cannot
  serve the popover: the catcher is anchored to all four edges and would blur the entire output. The
  protocol region is the only route that blurs exactly what is painted.
- **niri does not round a protocol region.** Rounded corners need stepped one-pixel rows per corner at
  the CSS radius; a single rectangle shows blur past each corner.
- **`compute_bounds` is the border box**, so a region built from it leaves `box-shadow` and margins
  unblurred — measured, the wallpaper under the panel's shadow stays sharp.
- **GTK's own connection is reachable without a new crate**: `gdk_wayland_display_get_wl_display` and
  `gdk_wayland_surface_get_wl_surface` declared `extern "C"`, then `Backend::from_foreign_display` under
  `wayland-client`'s `system` feature. That feature adds `dlib` and `scoped-tls` to the lockfile, and
  through feature unification moves every `wayland-client` user in the workspace — sunset, idle, the
  picker, the clipboard backend — onto libwayland's C backend.
- **Create the effect on `map` and destroy it on `unmap`.** `set_blur_region` on a destroyed surface
  is a protocol error, and on a borrowed connection that kills GTK's whole display.
- **Update the region in the frame clock's `layout` phase.** It is double-buffered and then rides the
  same commit as the new size, so a resizing popover never shows the old region for a frame.
- **niri defaults a layer surface to xray** — the blur samples the wallpaper only, skipping the windows
  between. Measured on the popover over a browser. `background-effect { xray false; }` in a
  `layer-rule` is the only switch; the protocol carries a region and nothing else.
- **`WidgetPaintable` does not draw the widget — it hands back the render node of its last paint**
  (`gtkwidgetpaintable.c`, 4.22). A widget that has never painted reads as nothing, so corner radii are
  read after its first paint; reading them in the first layout would cache square corners for good.
  GTK 4.22 draws a rounded background as a `RoundedClipNode` around a color node
  (`gtkrenderbackground.c:62-68`), which is what the radius is read from.
- **A region sent outside GTK's frame is never committed unless glimpse commits it.** The region is
  double-buffered; GTK commits only when it paints, and `queue_draw()` on a surface that has nothing
  new to draw produced no commit. Measured under `WAYLAND_DEBUG=1`: the popover's region went out
  200µs after the fade's last commit and reached niri 3s later, with the close — so the popover
  showed no blur while open, then blurred wallpaper through its whole fade-out. Grep a trace for
  `set_blur_region` and the next `wl_surface#N.commit` of the same surface before believing a
  region took effect.
- **A fading surface must drop out of the region.** Both the popover and the notification entries
  animate the widget `opacity`, and a blur cannot fade: the first live run showed a blurred block
  ahead of a card fading in and a ghost where one faded out. Opacity changes repaint without a
  relayout, so the region is also rebuilt after every paint. Waiting for *full* opacity was wrong the
  other way: a 120fps `wf-recorder` capture showed the finished content over a sharp background for
  six frames before the blur arrived. At half opacity the blur arrives about 30ms into the 150ms
  ease-out fade, while the content is still faint. `wf-recorder -o <output> -g <region> -r 120` and one frame at a time from
  `ffmpeg` is how to judge any of this; `grim` in a loop is too slow to see a frame.

**GTK 4.22 cannot hand a CSS value back to code, September 2026.** Read out of the installed
headers and the 4.22 source, and probed. The only public getter for a computed style value is
`gtk_widget_get_color`; nothing returns a length, a duration or a custom property. The deprecated
`gtk_style_context_to_string(SHOW_STYLE)` looks like a way in and is not: a regular property such as
`transition-duration` is printed only when the style kept its CSS section, which
`gtkcssprovider.c` does only under `GTK_CSS_DEBUG` or the Inspector, and a custom property is
printed as the text it was declared with, `var()` unresolved, and stale until the next restyle. A
value Rust and CSS both need therefore lives in Rust and Rust writes it into a sheet: `[appearance]
animation-speed` scales the base duration and `Styles` publishes the result as `--gl-duration`, the way
KDE scales Kirigami's durations by `AnimationDurationFactor` and Cinnamon its window effects by
`window-effect-speed`.
Corner radii are the exception only because GTK draws them into render nodes.

**The `#[ignore]`d GTK suite is not run by `just verify`, September 2026.** `just test` is
`cargo test --workspace` with no `--include-ignored`, so `tests::widgets` — the single function
holding nearly every widget assertion — only runs under `just test-compositor`, and the suite can
rot unnoticed between compositor runs. One failure there hides every assertion after it in the same
function.

**Splitting that function into more `#[ignore]`d tests does not fix it and is unsound, September
2026.** `gtk4::init()` **panics** when GTK is already initialized on another thread — *"Attempted to
initialize GTK from two different threads"*, `gtk4-0.11.4/src/rt.rs:138` — so the
`if gtk4::init().is_err() { return; }` guard every one of them opens with can never fire. Cargo gives
each `#[test]` its own thread **including under `--test-threads=1`**, so in one test process only the
**first** GTK test can pass. Measured on the same tree: `--test-threads=1` gives 59 passed and 6
failed, five of them on that panic; the default parallel run got lucky and all six ran. Which
assertions execute is therefore decided by thread scheduling, and a green run is not evidence they
ran at all. Six tests are in this state today — bead `glimpse-9vjo`. Until it is resolved, **do not
add a seventh**: a new widget's assertions go at the end of `widgets()`, and the constraint is per
*process*, so a separate test binary under `tests/` is the escape hatch if one is really needed.

**A separate `#[ignore]`d GTK test can pass without running, September 2026.** Every one of them
opens with `if gtk4::init().is_err() { return; }`, and cargo runs them on parallel threads: the one
that initializes GTK first wins, and the rest return early and report `ok`. Measured on
`workspace_name_popover_widgets` — a mutation that clobbers the entry passed inside
`just test-crate-compositor glimpse-widgets` and failed when the test ran alone. A mutation check
on a GTK test runs that test by itself (`just test-one <crate> tests::<name>`).

**niri refuses a workspace name another workspace holds, and says `Ok`, September 2026.** Measured
on niri 26.04: `set-workspace-name` naming a second workspace after the first exits 0, changes
nothing and emits no event. A client that updates optimistically must drop its guess on any reply
and re-read the snapshot, or the refused name stays on screen with nothing to correct it — which is
what the workspace-name applet does.

**A GTK test run beside another GTK test passes without running, September 2026.** libtest gives
every test its own thread, and `gtk4::init()` answers `Err` on any thread but the first one to
initialize, so every `if gtk4::init().is_err() { return; }` after the first reports `ok` having
asserted nothing. Measured on `glimpse-widgets`: under `just test-crate-compositor glimpse-widgets`,
four deliberate mutations of `PasswordPrompt` all passed, and the failing tests changed from run to
run (`widgets`, `clipboard_widgets`, `removable_popover_widgets`, `printing_popover_states`); run
alone, all four mutations failed. `just test-compositor` and `just test-crate-compositor` now run
the non-ignored suite once and then run every `#[ignore]`d test in its own `cargo test` process —
listed with `cargo test -p <crate> -- --list --ignored`, each invoked as `cargo test -p <crate>
<name> -- --ignored --exact` — so a green run is evidence every one of them actually ran, not a
single survivor reporting for the rest. Both recipes refuse to start when neither
`WAYLAND_DISPLAY` nor `DISPLAY` is set, since without a display every GTK test would return early
and pass the same vacuous way.

**`SwitchRow`'s gesture behaviour is asserted only in part, September 2026.** The headless test
proves one emitter — the row body and a programmatic knob change each produce exactly one `toggled`.
It cannot prove the **pointer** case: `Row` is a `Gtk.Button` and `Gtk.Switch` runs its own click
gesture inside it, so whether a press on the knob is claimed by the switch or also reaches the
button's `clicked` needs a mapped surface. That one is on the manual list, not the test list.

**No battery is not a rendering bug, September 2026.** `render::state_of` already puts a percentage
in a connected device's row whenever BlueZ reports one. Measured here: a *connected* WH-1000XM4
exposes no `org.bluez.Battery1` interface at all, and `/etc/bluetooth/main.conf` ships
`#Experimental = false`. BlueZ derives headset battery from the Apple HFP extension, which that flag
gates. Enabling it is a system change and the user's call; there is nothing to fix in glimpse.

**A `Gtk.PopoverMenu` renders here perfectly well; three things make it look as if it does not.**
A popover's anchor rectangle is its *parent's allocation*, so a parent filling the window anchors the
popup to the window's own edge — with a bottom gravity on a full-height window the compositor
squeezes it to a few pixels and GTK tears it down. A compositor also dismisses a popup belonging to
an **unfocused** window with `xdg_popup.popup_done` the moment it appears, and a preview opens on its
own workspace, which is usually not the focused one. And `active: true` on a `Gtk.MenuButton` in a
`.blp` does nothing at all: `Builder` applies it before the button is realized and `GtkMenuButton`
drops it, so no `xdg_positioner` ever reaches the compositor. `WAYLAND_DEBUG=1` and the
`set_anchor_rect` / `configure` pair tell the three apart in one run.

**A `Gtk.Stack` with `interpolate-size` under-allocates its taller page, September 2026.** The
bluetooth popover's two pages measure 136px and 384px tall; interpolating between them makes the
stack report a height below the taller page's minimum and then allocate it that height, and
`gtk_widget_measure` warns on every frame — *"Trying to measure GtkBox for height of 136, but it
needs at least 384"*. `vhomogeneous: false` does not help, because the interpolation is the thing
allocating. A crossfade with no size interpolation is the fix; the height snaps, which is also what
a popover the compositor re-places on each measurement change wants.

**`Status` is two properties on two interfaces.** `org.kde.StatusNotifierItem.Status` is
`Active`/`Passive`/`NeedsAttention` — and `Passive` is a placement instruction, the host tucks the
item away, not a tint. `com.canonical.dbusmenu.Status` is `normal`/`notice`, on the menu object.
Both read their quiet value on every live item, so nothing on screen separates them.


**NetworkManager 1.58.1 on this machine, September 2026.** Introspected and probed live; the full
account is `var/network/research.md`.

- **`ObjectManager` is at `/org/freedesktop`** — not `/` (where BlueZ puts it) and not
  `/org/freedesktop/NetworkManager`, both of which answer `UnknownMethod`/`UnknownInterface`. A
  `path_namespace` copied from the bluetooth source matches nothing.
- **`Devices` and `AllDevices` return the identical list**, `Managed=false` veths included, so
  filtering cannot be delegated to the choice of property. **9 devices, one user-facing**: `wlp99s0`
  (type 2). The rest are `docker0` and two `br-*` (13), two `veth*` (20, unmanaged), `p2p-dev-wlp99s0`
  (30), `lo` (32) and a Bluetooth NAP (5) whose **`Interface` is a MAC address, not a netdev name**.
  There is **no ethernet device and no modem**.
- **5 active connections, 4 of them Docker plumbing and `lo`.** A popover listing
  `ActiveConnections` naively shows all five.
- **`Device.StateReason` is `(uu)`**; element 0 repeats `State`, element 1 is the reason.
  **`Connection.Active` has no `StateReason` property at all** — `GetAll` returns thirteen members
  and none of them is it. The reason arrives only on `StateChanged(state, reason)`, so a client must
  cache `path → reason` and **evict on state 4**, or a recycled path serves a stale one.
- **What actually churns is `Bitrate`, not statistics.** Thirty idle seconds produced two
  `AccessPoint.Strength` changes and one `Device.Bitrate`. `Device.Statistics` is present on the
  wireless device but `RefreshRateMs` is **0**, so `RxBytes`/`TxBytes` emit nothing until some client
  sets the rate — it is writable by anyone on the bus, so the relevance allowlist still excludes it.
  `Strength` cannot be excluded because it is needed. Banding it in the model would suppress most
  republishes, but the tooltip shows the exact percentage, so the **raw** value is what the state
  carries and the band is derived at render. Measured cost of that choice: **12 republishes in 75
  seconds** on an idle connected machine, each one a `Vec<IndicatorSpec>` rebuild whose widget
  setters all compare before writing. That is the price of an honest tooltip, and it is small.
- **14 access points, 11 distinct SSIDs.** Three SSIDs carry two BSSIDs each, so no dedup means three
  duplicate rows. Dedup picks the strongest **whole AP**, never merged fields: `RubinowyKlon1`
  advertises `Flags` 3 on its stronger BSSID and `Flags` 1 on the weaker, so a merge invents a
  security level. **The connected AP is not the strongest** — `Skylink` at 70 is chosen over a
  94 — which makes the connected row a placement decision rather than a sort key.
- **One AP beacons a zero-byte SSID.** `Ssid` is `ay` and need not be valid UTF-8 or non-empty.
- **WPA3/SAE (0x400) is on this network and enterprise 802.1X (0x200) is not**, so only the
  enterprise badge is unverifiable here — `KHARKIV` and two unnamed APs advertise SAE. The common
  case is `RsnFlags` 392 (`0x188` = `KEY_MGMT_PSK | GROUP_CCMP | PAIR_CCMP`); one router runs mixed
  WPA1+WPA2 (`WpaFlags` 324, `RsnFlags` 332).
- **A VPN is testable here after all, and needs neither root nor a peer.** `nmcli connection add type
  wireguard con-name <name> ifname <iface>` plus a generated `wireguard.private-key` is a profile
  NetworkManager activates on its own — the interface comes up, `Vpn` rows and the second chip render,
  and both are ordinary polkit operations for an active session. Nothing presents as **ethernet**
  (`DeviceType` 1) without hardware: a veth is 20, a dummy 22, a tun/tap 16, and all three are
  filtered by design.
- **7 saved connections, 2 of them user-facing Wi-Fi**; four are generated bridge/loopback profiles
  and one is a Bluetooth PAN, and **none of them is a VPN** — make one, per the bullet above.
  **`autoconnect` is absent from a profile when it is true** — reading `Option<bool>` and
  treating `None` as false inverts both Wi-Fi profiles.
- **A saved Wi-Fi profile carries `key-mgmt` and no `psk`.** Secrets come only from `GetSecrets` or
  an agent. `seen-bssids` ties a profile to an AP cheaply; `VersionId` is a change token.
- **`RegisterWithCapabilities(su)` exists on `AgentManager`** beside plain `Register(s)`.
  `_old/glimpse-shell/src/agents/network.rs:254` called the plain one and so never received VPN
  secret hints.
- **`gdbus` prints `y` values in hex** (`<byte 0x45>`). A research script parsing `\d+` reads every
  signal strength as 0. Nothing about the wire; everything about the tool.

**What the network epic itself measured.** Every reason code was checked against
`/usr/include/libnm/nm-dbus-interface.h` rather than carried over, and two that `_old` had wrong are
the ones to get right: device reason **39 is `USER_REQUESTED`**, not a dependency failure, so a
deliberate disconnect must not be reported; and `NMActiveConnectionStateReason`'s
**`USER_DISCONNECTED` is 2**, while 3 is `DEVICE_DISCONNECTED`. A reason the classifier does not
recognise is a failure, never `Ok` — it is asked only after something has already failed.
**`NM_SECRET_AGENT_GET_SECRETS_FLAG_REQUEST_NEW` (0x2) implies interaction is allowed**, stated in
the header beside the constant, so testing `ALLOW_INTERACTION` alone drops every wrong-password
retry in silence.

**`nmcli` is its own secret agent and will not delegate, September 2026.** `nmcli connection up` on
a profile with no stored secret answers *"password is required"* and refuses to ask without
`--ask` — it never reaches another registered agent, so it cannot drive a glimpse secret prompt. Ask
NetworkManager directly instead: `gdbus call --system --dest org.freedesktop.NetworkManager
--object-path /org/freedesktop/NetworkManager --method
org.freedesktop.NetworkManager.ActivateConnection <profile> <device> /`. A profile whose stored
password is wrong is the cheapest way to reach the `REQUEST_NEW` retry: NM tries the stored one,
fails, and asks again with the retry flag set.

**A layer-shell popover can be typed into, but only if it asks, September 2026.** The popover catcher
takes `KeyboardMode::None`, which is why the first secret prompt was a window on the host instead of a
page in the popover. `KeyboardMode::OnDemand` while a prompt is up — and `None` again on close and on
every open — gives the entry a focus ring under niri and costs the popover nothing when no entry is
there. **`ydotool` still cannot drive it**: with the entry focused and `ydotoold` running, `ydotool
type` exits 0 and delivers nothing, the same as its clicks. Keystrokes reaching a real entry stay on
the manual list.

**Disconnecting a network is not a stable test state, September 2026.** `nmcli connection down` on
an autoconnecting profile is undone by NetworkManager itself within seconds, so a test built on it
measures the race rather than the code; and after a radio cycle NM reconnects to the **strongest**
saved profile, which is not necessarily the one it was on. `scripts/net-guard.sh` — `just net-guard
baseline|arm|status|restore|disarm` — is the safety net: it guards **connectivity**, not one
profile, runs detached under `systemd-run --user` so it survives the shell that armed it, and gives
three ten-second grace periods before restoring. Arm it before any disruptive network test.

**A states board does not fit one screen, and `niri msg` under-reports a floating window's width,
September 2026.** `bluetooth_states.blp` renders 483x2250 and `network_states.blp` 498x2495, against
an eDP-1 of 1440 and a DP-2 of 1728 logical pixels. **A single `grim` cannot capture a whole board**
and the part below the fold is not merely cropped — it is never composited, so it captures as blank.
Review a board by opening the preview, and verify the lower states by rendering a temporary copy that
holds only them. Separately, `window_size` from `niri msg -j windows` is **narrower than the window
actually paints** for these floating previews — cropping to it silently cuts the right-hand column,
which is where `$Section`'s `count` sits. A count that looks missing is almost always this and not the
blueprint; pad the region before concluding anything.

**`network-error-symbolic` is not colour-neutral, September 2026.** Of the network glyphs Adwaita
ships, it is the only one carrying `class="error"` and a baked `fill="#e01b24"`;
`network-offline-symbolic`, `network-wireless-offline-symbolic`, `network-no-route-symbolic` and
`network-wireless-no-route-symbolic` are all plain `#2e3436` and recolour with the CSS `color`
property as a symbolic icon should. **An indicator takes no colour**, so a state that must not be
tinted uses one of the neutral four — the `-symbolic` suffix alone does not promise it.

**Audio on this machine, September 2026.** Of three applications playing on a live session, only
one advertised `application.icon_name`. **`application.name` arrives wrapped as `PipeWire ALSA
[zed-editor]` for an ALSA-compat client**, and **`application.process.binary` is
`WebKitWebProcess` for a WebKit-hosted application** — so the binary is not a usable display name
and neither field alone identifies the app; an icon ladder that tries several sources is mandatory
rather than polish.

`SourceOutputInfo` is field-for-field symmetric with `SinkInputInfo` — `corked`, volume, mute,
`has_volume`, `volume_writable` and `proplist` all mirror — and `move_source_output_by_index`,
`set_source_output_volume` and `set_source_output_mute` all exist beside their sink-input
counterparts, so capture cost almost nothing beyond playback: one shared mapping, one `Direction`
match at each call site. **`SourceInfo.monitor_of_sink` identifies a monitor structurally**;
matching `.monitor` on the device name is a guess this field already answers.

**libpulse-binding 2.30.1 has an inverted null test in `Operation::from_raw`**, so `saved_cb` is
`None` for every real callback and a bare `cancel()` frees nothing. `cancel()` is guarded by
`get_state() == OperationState::Running`, taken under the mainloop lock — a fix upstream turns an
unguarded `cancel()` into a heap double free, and the workspace pin (`libpulse-binding = "2.30.1"`)
is a caret requirement, so a routine dependency update could pull the fix out from under the guard
with no warning of its own.

Adwaita's `audio-volume-*` and `microphone-*` symbolics are all plain `#2e3436` with no baked
accent, unlike `network-error-symbolic` above. But **`.indicator--notice` paints an accent pill
behind the icon, so it cannot mark a quiet state such as muted, and `.indicator--info` does not
exist at all** — a muted or unavailable device needs a signal other than either indicator class.

**GLib caches its desktop-file search path at the first `DesktopAppInfo` use and never re-reads
`XDG_DATA_HOME` afterward**, so a hermetic per-test override is unsafe in a shared test process —
whichever test touches `DesktopAppInfo` first decides the path for every test that runs after it
in the same process.

**Brightness sources, September 2026.** Measured against an amdgpu panel backlight and an ASUS
`asus::kbd_backlight` (UPower 1.91.3, kernel udev 261).

- **The `backlight` uevent fires on every WRITE, not every change** — three `SetBrightness` calls
  through logind produced three `change` uevents, and the first wrote the value already present. A
  udev source must read a uevent as "re-read this device", never as "something external moved";
  what stops the loop is structural, not a guard — the reconcile path has no write edge back out.
- **`udev` is promoted to a direct workspace dependency solely to enable its `send` feature**, which
  `tokio-udev` does not request; without it, `Device`/`MonitorSocket`/`Builder`/`Udev` are `!Send`
  and the `Sub::stream` plumbing does not compile. The resulting `unsafe impl Send` is sound only
  because no `udev::Device` or `Event` ever escapes the single stream task while the monitor lives.
  `libudev` is now a link-time system dependency, confirmed present here at version 261 via
  pkg-config.
- **UPower's `KbdBacklight` introspection under-reports, the mirror of the BlueZ case above**: the
  parent node's introspection lists the interface with only a `NativePath` property and no methods,
  yet `GetBrightness`/`GetMaxBrightness` both answer on it. What actually breaks is that the
  parent's `NativePath` is the EMPTY STRING; only a child node (machine-specific, e.g.
  `asusookbd_backlight` here) carries the real sysfs path, which is the dedup key against
  `/sys/class/leds`.
- **`BrightnessChangedWithSource` does not discriminate a client's own write.** Calling
  `SetBrightness` through UPower's own D-Bus API reports `(value, "external")` — indistinguishable
  from an actual external change. Filtering on `source` filters nothing; the echo loop is stopped
  the same structural way as the backlight uevent.
- **Adwaita ships no `display-brightness-*` level variants** — there is no icon ladder to step
  through the way `network-*` or `audio-volume-*` have one.

**The night light Schedule trap, September 2026.** `NightLightState::active()` is `self.temperature
!= DAY`, so it reads `false` — "off" — for every daylight hour under `Automatic`, since the ramp
holds `DAY` (6500 K) whenever the sun says so. A switch or an icon bound to `active()` lies all day;
the schedule to read is `schedule != "off"`.

**Output power and richer output info, September 2026.**

- **niri STORES the output scale rather than recomputing it** — three samples of `niri msg -j
  outputs` 0.4s apart returned a byte-identical `logical.scale` (1.25, exactly representable in
  `f64`) and an integer-mHz `refresh_rate`, which is what makes float equality safe inside
  `OutputInfo`'s republish gate. Hyprland's `logical.scale` is its own `f64` passed verbatim and is
  UNVERIFIED — no Hyprland session exists on this machine.
- **A niri output with `current_mode: null` has no `wl_output` global.** `gdk_monitor(connector)`
  finds nothing, `place()` calls `set_monitor(None)`, and the compositor picks — so a notification
  popup placed on a disabled output is NOT invisible, it lands on the compositor's default output,
  ignoring both `[notifications] monitor` and the focused-output rule, with no log line;
  `constrain_height`'s overflow trim also never runs, since it early-returns on the same `None`.
- **niri-ipc 26.4.0 declares `Action::PowerOffMonitors` as an EMPTY STRUCT variant and
  `Request::Action` as a NEWTYPE variant**, so the wire bytes are exactly
  `{"Action":{"PowerOffMonitors":{}}}` — read off the vendored crate source, not guessed.
- **Hyprland's `dispatch dpms off` wake-on-input is UNVERIFIED** — no Hyprland session exists on
  this machine to test it — and hypridle's own configuration pairs that dispatch with an explicit
  `on-resume = hyprctl dispatch dpms on`, which is how you configure something that does not come
  back by itself. `Capabilities.output_power` stays true on both backends for exactly this reason
  (bead `glimpse-50pe`): if Hyprland needs an explicit power-on, the two backends disagree.
- **EDID descriptor strings terminate with `0x0A` and pad with `0x20` — but some panels pad with
  `0x00`**, and `'\0'` is not whitespace. Sanitizing control characters must run BEFORE trimming,
  never after, or the null padding survives. **`make`, `model` and `serial` all come from the same
  EDID block and all reach a label** — `label_of` composes `format!("{make} {model}")`, so
  sanitizing only `model` leaves half of a rendered string raw.

**`zvariant`'s `OwnedValue` derive makes every D-Bus struct property APPEND-ONLY, September 2026.**
The generated impl does `Structure::try_from(value)?.into_fields()` then pops ONE FIELD PER DECLARED
FIELD in order, silently ignoring whatever is left over — so appending a field to a struct signature
is free (an old client's fewer downcasts all succeed, the extras are dropped), reordering breaks
decoding for any untyped client such as `gdbus`, and removing a field PANICS the client outright,
because the pop is `Vec::remove(0)`. None of this is visible from the D-Bus spec, which carries no
field names in a struct signature at all — only from the generated code.

**glib-rs panics on a NaN write to ANY `f64` GObject property, September 2026.**
`g_param_value_validate`'s `CLAMP` leaves a `NaN` value unchanged — `NaN > high` and `NaN < low` are
both false — but glib-rs's own `set_property` compares before and after with `!=` to decide whether
the value was coerced, and `NaN != NaN` reads as "changed"; with `LAX_VALIDATION` unset (the
default), that panics as "invalid or out of range" before the property's own setter body ever runs.
No paramspec bound — `minimum`, `maximum`, anything — prevents it; the guard has to be in the caller.

**Every brightness fixture used a 0-100 range, which hid a real bug, September 2026.** `$Fader`
carries the hardware's native range, so a display source arrives as `value` against a `maximum` of
`400000` on this machine's `amdgpu_bl1`. The popover readout printed the raw value and appended
`%`, reading `400000%` on a real panel. No test and no states board could fail: every `Source`
fixture and every board sets `maximum: 100`, where the raw value and the percentage are the same
number. A percentage assertion is only meaningful against a maximum that is not 100.

**`ddcci-backlight` is not loaded on this machine, so there is no external display source.**
`/sys/class/backlight` holds `amdgpu_bl1` alone. An absent external fader and a scroll that only
ever moves the built-in panel are both this, not applet bugs — `render::current_display` already
prefers the focused connector and falls back to the internal display, and with one source the
fallback is the only path. Load the module before concluding anything about multi-source brightness.

**Places: five sources, measured September 2026.** Full account in `var/places/research.md`.

- **UDisks2's `ObjectManager` is at `/org/freedesktop/UDisks2`** — a *third* location. BlueZ uses
  `/`, NetworkManager `/org/freedesktop`, and `/` here answers *"Object does not exist at path"*. A
  `path_namespace` copied from either sibling source in `glimpse-services` matches **nothing**: you
  enumerate fine, never receive a signal, and every headless test still passes.
- **`Filesystem.Size` is 0 for vfat and exfat** — the two commonest removable filesystems — while
  ext4 and btrfs answer. So UDisks2 **cannot** supply a capacity readout for the devices this
  applet exists to show. `Block.Size` is the total; free space is a `rustix::fs::statvfs` sample,
  which is a blocking syscall and belongs in `spawn_blocking`. There is no D-Bus signal for free
  space anywhere, so the interval poll is the only source; it is declared only while something is
  mounted, so a machine with no stick attached runs no timer.
- **`Drive.Media` is a closed 33-value vocabulary** (not 32 — counted programmatically out of
  `udisksd`), and it is the whole icon map.
- **Nothing needs a polkit policy.** `filesystem-mount`, `eject-media`, `power-off-drive` and
  `encrypted-unlock` are all `implicit active: yes`. The two that are `auth_admin_keep` —
  `filesystem-mount-system`, `filesystem-unmount-others` — are exactly what the `HintSystem` filter
  excludes, so the applet never reaches an agent prompt.
- **A loop device can never reach a removability filter.** It has **no `Drive` object at all**
  (`Block.Drive` is `/`) and `HintSystem` is true, so `udisksctl loop-setup` exercises UDisks2 but
  never the applet. Testing removable media needs `scsi_debug removable=1` (root) or real hardware,
  which is what `scripts/removable-test.sh up|down|status` wraps — a RAM-backed removable drive with
  two formatted partitions, plus `--optical` for the no-media state and `--readonly` for the
  read-only icon; it stays out of the `justfile`, which is the build workflow rather than a wrapper
  for fixtures. `mkfs.vfat` is **not installed** here; `mkfs.exfat` and `mkfs.ext4` are.
- **`pkexec` cannot authenticate on this machine, and the on-screen failure is misleading.** No
  polkit agent is registered for the session, so pkexec's textual fallback takes the password, PAM
  accepts it, and polkitd then refuses with `No session for cookie` — which prints as
  *"AUTHENTICATION FAILED ... Not authorized"* and reads exactly like a wrong password. The user is
  in `wheel` and `50-default.rules` grants it, so authorization is not the problem. Become root with
  `sudo` from a terminal, which is what the `justfile`'s `elevate` already defaults to and what
  `scripts/removable-test.sh` uses.
- **`glib::UserDirectory` is a closed enum of 8 against an open file format.** `user-dirs.dirs`
  accepts any `XDG_<NAME>_DIR`; this machine has **nine** keys, the ninth being `XDG_PROJECTS_DIR`,
  which `xdg-user-dir PROJECTS` resolves and `glib::user_special_dir` cannot see. Parse the file.
  `g_get_user_special_dir` also caches — `glib::reload_user_special_dirs_cache()` exists because it
  does — and `glimpse-services` declares no `glib` at all.
- **GTK4 reads `~/.config/gtk-3.0/bookmarks`**, not a gtk-4.0 path; confirmed out of
  `libgtk-4.so.1`, where `gtk-3.0` and `.gtk-bookmarks` sit adjacent in the bookmarks manager. The
  format is `<URI> <optional label>`, one per line. **`GBookmarkFile` is the wrong API** — it parses
  XBEL, a different format entirely.
- **Count `Trash/files`, never `Trash/info`.** Measured here: 363 against 1028, of which 665 are
  orphaned `.trashinfo`. Counting `info/` — which is what "read the .trashinfo files for the
  original paths" invites — reports **2.8x the truth**, and one `read_dir` of `files/` is the count.
- **`gio::VolumeMonitor` is unreachable from a service.** `glimpse-services` declares `gio-unix`
  only, with no `gio` and no `glib`, and `VolumeMonitor` is a singleton emitting on a thread-default
  main context — the opposite shape from the one-shot `DesktopAppInfo` lookup that is the existing
  precedent. Reading `$XDG_RUNTIME_DIR/gvfs` directly needs no dependency.
- **`ConnectionBus` is NOT verified on this machine.** Every drive answers `""` and `udevadm` shows
  these NVMe drives carry no `ID_BUS`; only `ieee1394` and `scsi` appear as standalone strings in
  `udisksd`. `usb` and `sdio` in the removability filter come from documentation, not measurement.
  `Removable` and `MediaRemovable` are the two arms that are certain.

**Privacy applet coverage, September 2026.** Every source was measured live against a real Chrome
session holding the camera, two mic captures and a DP-2 screen share at once, plus a `/proc` census
of 766 processes and a probe of niri's cast reporting against the `RemoteDesktop` portal.

- **PipeWire is blind to raw V4L2 capture, and Chrome is the case that proves it.** With Chrome live
  on the camera, the `Video/Source` node stayed `suspended` and no stream node appeared, while
  `/proc` reported `comm=chrome` holding `/dev/video0`. `_old` shipped a PipeWire-only camera
  detector and therefore could not see Chrome. **`libspa-v4l2.so` is mapped into the `pipewire` and
  `wireplumber` processes only**, never a client, so the daemon opens `/dev/videoN` — which is why a
  `/proc` fd scan also catches PipeWire-mediated capture, but names the holder `pipewire`. The scan
  itself costs **28-33 ms**; an early 10 s figure was bash forking `readlink` 766 times, not the
  syscall cost.
- **A camera open is not a camera stream, and a root process is invisible.** Both `/proc` and the
  `uvcvideo` refcount rise on a bare `open()` with no capture under way, which is why the wording is
  "in use" and never "recording" — and 479 of the 766 processes on this machine belong to another
  user and cannot be read, so a root process holding the camera is invisible outright.
- **Chrome opens TWO mic source-outputs on the real mic, so usages are deduplicated by app.**
  `application.name` arrives as `"Google Chrome input"`, not a display name, and
  `application.process.binary` is `chrome` — but preferring the binary regresses the WebKit case,
  where `identify.rs:263` asserts `name == "walz"` against a binary of `WebKitWebProcess`. **A
  monitor source can be `RUNNING` with nothing capturing it** — a bluez monitor was, during
  measurement — and capturing a monitor is system-audio recording, not microphone use, so it is
  excluded; a corked capture and an app id on the volume-control blocklist are excluded the same way.
- **`pipewire-alsa` is installed and the default ALSA PCM is `type pipewire`** —
  `/etc/alsa/conf.d/99-pipewire-default.conf`, confirmed present on this machine — so
  `scripts/privacy-test-mic-alsa`, which runs `arecord` with no `-D`, does NOT bypass PipeWire as its
  name implies. A genuine bypass needs `arecord -D hw:0`; whether such a capture is visible to the
  mic detector is unverified.
- **niri reports `pid: null` on a real portal-mediated cast**, measured against a live Chrome share;
  `target` and `session_id` are populated and reliable, and `pw_node_id` is the join key. It reports
  two cast kinds, `PipeWire` and `WlrScreencopy`; screencopy without damage tracking is treated as a
  screenshot and not reported, which is why `grim` does not light the indicator. **A `WlrScreencopy`
  cast carries no app name** — there is no PipeWire node to attribute.
- **Chrome tab sharing is invisible, and it is a known limitation, not a bug to chase.** Sharing a
  tab rather than a screen or a window is captured inside Chrome and creates no portal session and
  no compositor cast; `_old` hit this as a P1 and could not solve it either. Screen and window
  sharing are unaffected.
- **Remote-desktop input injection is undetectable, and is the same kind of limitation.** The screen
  half is covered — wayvnc binds wlr-screencopy, which niri reports as an ordinary
  `CastKind::WlrScreencopy` cast — but virtual pointer and keyboard are Wayland protocols with no IPC
  reporting who bound them, a client cannot enumerate another client's bindings, and no service crate
  may take a Wayland dependency. Measured: **niri owns `Mutter.ScreenCast` but NOT
  `Mutter.RemoteDesktop`**, and xdg-desktop-portal-gnome exposes only `impl.portal.ScreenCast`, so
  `RemoteDesktop.AvailableDeviceTypes` is 0.
- **`GeoClue2.Manager.InUse` is a global boolean with no per-client attribution anywhere**, so
  location never names an application — this is a property of GeoClue, not a gap in the service.
- **At full service shutdown a detached GeoClue release can lose a race and skip `Stop`/
  `DeleteClient`.** Harmless — the process is exiting and GeoClue detects the connection loss
  itself. The two cases that matter, a config switch to Manual mid-wait and a refresh mid-wait, take
  the hard-abort path instead and are covered by
  `a_source_torn_down_mid_wait_still_runs_its_release_effect`.
- **The privacy `show_*` flags are render filters only.** They do not stop the service's own
  sources, so `show-location = false` still lets the service talk to GeoClue.

**KDE Connect on this machine, September 2026.** `kdeconnectd` 26.08.1 against a Pixel 10 Pro
(Android, protocol 8), introspected and traced live.

- **`busctl introspect` rejects `/modules/kdeconnect`** — the daemon declares
  `sendSimpleNotification` twice. `gdbus introspect` still prints it. The proxies in
  `glimpse-dbus` are hand-written and every method carries `#[zbus(name = "camelCase")]`.
- **It emits no `PropertiesChanged` at all.** Pairing produced `pairStateChanged(i)` (0 → 1 → 3),
  `statusIconNameChanged`, `pluginsChanged`, `deviceVisibilityChanged(s,b)`, `deviceListChanged`
  and `battery.refreshed(b,i)`, and nothing else.
- **An installed daemon autostarts under niri** through
  `app-org.kde.kdeconnect.daemon@autostart.service`, pulled in by `xdg-desktop-autostart.target`.
  It is also D-Bus activatable, which is why the service calls the owner's unique name only.
- **A paired device survives unreachable; unpairing an unreachable one deletes its object path**,
  and plugin objects exist only while a device is paired and reachable.
- **A key that no longer matches its certificate stalls every TLS handshake silently.** The only
  symptom is `Host timed out without sending any identity` in the journal after the phone's
  plaintext identity arrives. `openssl x509 -pubkey` against `openssl pkey -pubout` on
  `~/.config/kdeconnect/*.pem` tells it apart in one command; moving both aside regenerates the
  identity, and every phone must pair again.
- **Phone-to-laptop traffic needs the Android app's own permissions.** After a storage wipe, a
  laptop-to-phone ping arrived while the phone's ping and notifications never reached the daemon.
- **The battery plugin's `iconName` is the low-battery signal** — `battery-{full,good,low,caution,
  empty}[-charging]-symbolic` — so the applet reads `caution`/`empty` rather than a threshold of
  its own.
- **`connectivity_report` answers `''` and `-1` on this phone** while the plugin is loaded and the
  phone is on a cellular network, so nothing about mobile signal has been seen with a real value.

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

<!-- BACKLOG.MD GUIDELINES START -->
<!-- backlog.md-instructions-version: 1.52.0 -->
<CRITICAL_INSTRUCTION>

## Backlog.md Workflow

This project uses Backlog.md for task and project management.

**At the beginning of each conversation in this project, run `backlog instructions overview` before answering or taking action. Re-read it only if you have not read it yet in the current conversation.**

Use the overview to decide whether to search, read, create, or update Backlog tasks.

Before task lifecycle actions, read the matching detailed guide:
- `backlog instructions task-creation` before creating or splitting tasks
- `backlog instructions task-execution` before planning, changing status or assignee, adding a plan or implementation notes, or implementing task work
- `backlog instructions task-finalization` before checking acceptance criteria, writing final summaries, or moving tasks to terminal statuses

Use `backlog <command> --help` before running unfamiliar commands. Help shows options, fields, and examples.

Do not edit Backlog task, draft, document, decision, or milestone markdown files directly. Use the `backlog` CLI so metadata, relationships, and history stay consistent.

</CRITICAL_INSTRUCTION>
<!-- BACKLOG.MD GUIDELINES END -->
