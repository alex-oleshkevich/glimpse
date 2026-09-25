# Handbook

The long form of AGENTS.md: every convention, constraint and procedure with its reason.

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
├── docs/         agent reference: known state, live testing, preview host, translations
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

### Live runs

Full account: `docs/live-testing.md`. The three that break the user's session:

- **`direnv` exports `GLIMPSE_CONFIG_PATH`**, which beats a scratch `HOME` — pass `--config` and read
  the `load config path=` log line.
- **Give every test panel a fresh `GLIMPSE_PANEL_APP_ID`**, or it hands off to a running one and
  looks like a hang.
- **Never `pkill -x glimpse-panel`** — it kills the session bar. Kill the pid you started.

### Previewing a widget

`just preview <path/to/blueprint.blp> [fixture]` renders one blueprint with the real widgets and
reloads on save. How it works, its fixtures and its traps: `docs/preview.md` — read it before
touching the preview host or a `var/widget_examples/` board.

## Translations

One gettext domain, `glimpse`, owned by `glimpse-utils`. Mark a string where it is written —
`_("Text")` in Blueprint, `gettext`/`ngettext` in Rust — with double quotes and named `{placeholders}`
filled by `.replace`, never `format!` into the msgid. `just check-strings` is part of `just verify`;
after `just fmt` shifts lines, `just extract-strings` refreshes `po/glimpse.pot`. Locale init order,
units, adding a language and the extractor's traps: `docs/translations.md`.

## Work rules

- Work on one feature at a time, and only start the next after the current one passes end-to-end
  verification. Don't "also refactor" feature B while implementing feature A.
- **Every feature-level plan runs through `plan-precision` before it gets sliced into epics or
  issues.** Not just when a session happens to ask for planning help — any design/architecture
  doc, RFC, or epic brief for this repo. An epic sliced from a plan that skipped it ships the
  plan's own gaps as inventions, one per issue, discovered by a reviewer or a user instead of
  before code was written.
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
  `Wants=`/`WantedBy=` cannot, which is why `glimpse-session.target` starts it through `Wants=` alone.
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

- **A lesson becomes a rule, never a record.** A trap that would cause a bug goes into the owning
  crate's `README.md` as one present-tense rule, or into this file if it spans crates. Findings,
  measurements and open work go in beads. `AGENTS.md` stays a short index — never append to it.
- **Update the crate's `README.md` in the same change that alters what the crate does.**
- Remove instructions that stop being true rather than adding a caveat beside them. Two rules on the
  same topic produce worse behaviour than one.

**A crate `README.md` is capped at 300 lines, and states rules, not history.** Over the cap, cut
until it fits. Write the rule that is true now, in the present tense, and delete the one it
replaces. None of this belongs in a README: what the code used to do; bead numbers as evidence; what
you tested or chose not to test; a story where a clause would do. Keep what the thing is, the rule,
and the one consequence that explains why the rule exists. When a change makes a README longer, ask
which existing paragraph it replaces — a rewrite that only ever appends is a symptom. No measurements, no
dates, no current state of the machine or the backlog: work and its state live in beads.
