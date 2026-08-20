# glimpse

A desktop shell suite for Wayland compositors, targeting Niri first and Hyprland second: a panel, a
wallpaper renderer and a lock screen. One daemon (`glimpsed`) owns every piece of session state and
every OS integration; the UI binaries are stateless clients that render it over a single Unix
socket. When a rule below does not cover a situation, the deciding question is usually "who
owns this state?" — and the answer is almost always the daemon.

The repository is at the start of an implementation: crates exist as empty stubs, `specs/` describes
the target system.

## Specs come first

`specs/` is the source of truth, not the code. Read `specs/index.md`, then the specs that matter for
the task, before writing anything.

Any change to behaviour edits the affected spec first, appends a Changelog line, and sets its state
back to `draft`. Never leave a spec describing behaviour that does not exist. The `sdd` skill owns
this flow.

**Record why, not just what.** A decision that changes direction, reverses an earlier one, or that
somebody would plausibly re-propose in six months gets an entry in `specs/decisions.md`. A spec's
Changelog says what changed in that document; the decision log says why the project moved, which is
the part that is lost when a decision spans several specs. It is append-only — a reversal is a new
entry naming the old one, never an edit to it.

**No spec changes without approval.** Propose the edit — the exact wording, not a summary of it —
and wait for a yes. This holds for a one-word correction as much as for a new section, and it holds
when the spec is plainly wrong: a spec records decisions somebody made, so changing it silently
rewrites what they decided. Where the code and a spec disagree mid-task, stop and say so rather than
quietly editing either one to match the other.

## Prior art

This is the second generation of an application that already exists. Two bodies of earlier work are
in the tree, and both are **reference only — never a source of truth.**

| Where          | What it is                                                    |
| -------------- | ------------------------------------------------------------- |
| `_old/`        | the shipped previous implementation, in Rust                  |
| `var/glimpse2` | design documents for this rewrite, written by another agent   |

Read either when you want to know how a problem was solved before, what a backend actually does, or
which edge cases turned out to matter in practice. That knowledge is the reason they are kept: it
was paid for once already, and the specs are better for absorbing it.

Neither one decides anything. `_old/` is one answer among several and frequently the wrong one —
where it and `specs/` disagree, `specs/` wins, and if `_old/` is right the fix is to edit the spec
and say so. `var/glimpse2` was written without the constraints in this file and does not know what
has been decided since; treat it as a proposal from someone who has left the project.

**Never edit either, never build them, and never copy code out of them.** This is not a port. The
job is a smaller, simpler, cleaner application than the one in `_old/` — if a design lands at the
same size and shape as its predecessor, that is a signal to look again, not a sign of fidelity.

## Structure

```
glimpse/
├── crates/       all Rust code, flat, one directory per crate
├── specs/        numbered specs + index.md — the source of truth
├── data/         installed assets: systemd units, D-Bus service files, pam.d, default config
├── scripts/      development helpers, not installed — contents predate the rewrite
├── wallpapers/   bundled wallpapers
├── var/          scratch, not installed; `var/glimpse2` holds third-party design drafts
└── _old/         the previous implementation, kept for reference only
```

| Crate               | Role                                                              |
| ------------------- | ----------------------------------------------------------------- |
| `glimpse-ipc`       | wire frames, codec, `Topic` trait, payload types, errors, client, server |
| `glimpse-config`    | layered TOML load, drop-ins, merge, validate, watch               |
| `glimpse-services`  | service framework and every service implementation                |
| `glimpse-widgets`   | GObject subclasses, Blueprint templates, shared CSS               |
| `glimpsed`          | broker, `WaylandEdge` impl                                        |
| `glimpse-panel`     | panel and applets — builds the binary named `glimpse`             |
| `glimpse-wallpaper` | background layer surface, decode cache, transitions               |
| `glimpse-lock`      | `ext-session-lock-v1` surfaces, PAM                               |
| `glimpsectl`        | CLI and TUI                                                       |
| `glimpse-devtools`  | widget previewer, not installed                                   |

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
`.claude/rules/ui.md` for the GTK crates. General GTK4, libadwaita and relm4 craft is covered by the
`relm4`, `gtk4-styles` and `libadwaita-styles` skills. D-Bus work — every mirror service, plus the
two names glimpsed owns — is covered by the project-local `zbus` skill in `.claude/skills/zbus/`,
which carries introspected signatures for NetworkManager, BlueZ, logind, UPower, MPRIS,
StatusNotifierItem, dbusmenu and Notifications.

**Code**

- **Comment intent, never mechanics.** Name functions, types and variables so the *what* needs no
  comment, and write one only where the *why* cannot be read off the code: a constraint that is not
  visible locally, a simpler approach that was tried and does not work, an `#[allow(...)]` that
  needs justifying. A comment restating the signature is worse than none — delete it. Anything
  longer than two or three lines is rationale, and rationale belongs in the crate's `README.md`.
- **Never reference a spec from code.** No `specs/NNN` paths, no spec numbers, no "per the spec".
  Code states what it does; the specs are found from the READMEs.
- User-facing strings are not comments. `help = "..."` on a clap argument, an error message, a log
  line — all fine, and stripping them breaks output.

**Dependencies**

- Every crate dependency is inherited: `serde.workspace = true`. Add the version to
  `[workspace.dependencies]` in the root `Cargo.toml`, never to a crate manifest.
- `glimpse-ipc` takes serde and tokio and nothing else — no zbus, no GTK. Its `topics/` and
  `frame.rs` are the input to `schemars` for the Python, TypeScript and Go SDK types.
- Nothing depends on `glimpsed`. It is a leaf. Shared code goes in proto, client, config, services
  or widgets.
- A trait the framework needs from the daemon is declared in `glimpse-services` and implemented in
  `glimpsed` — `BrokerHandle`, `WaylandEdge` — each with a mock beside the declaration.

**Naming**

- Topics are `domain.name`, lower snake case, dots as separators: `audio.volume`,
  `tray.item.{id}.menu`
- Commands are `domain.verb_object`: `audio.set_volume`, `tray.menu_about_to_show`
- One config file, `config.toml`. Top-level table per owner: one table per service, named for the
  service, for the daemon; `[panel]`, `[wallpaper]`, `[lock]` for the UI
  binaries. A binary reads only the tables it owns. Stylesheets stay separate: `panel.css`,
  `lock.css`. Schema and layering: `specs/010_configuration.md`

**File placement**

| Kind of file                                            | Goes in                          |
| ------------------------------------------------------- | -------------------------------- |
| wire payload type                                       | `glimpse-ipc/src/topics/`        |
| service implementation                                  | `glimpse-services/src/services/` |
| anything touching a `wl_` object                        | `glimpsed/src/wayland/`          |
| anything touching GTK                                   | a UI crate or `glimpse-widgets`  |
| systemd unit, D-Bus service file, pam.d entry, defaults | `data/`                          |

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
just lint            # clippy, warnings are errors
just test            # headless tests
just fmt             # format in place
just test-compositor # also runs the #[ignore] Wayland tests; needs a compositor
just check-units     # systemd-analyze verify on the shipped units
```

Running a binary goes through `just run-daemon`, `just run-panel`, `just run-wallpaper`,
`just run-locker`, `just ctl <args>`, `just devtools <args>`. `just nested` opens a nested niri
window for a dev loop that does not disturb the running session.

A recipe that is missing or wrong gets fixed in the `justfile`. Do not work around it with a raw
cargo invocation.

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
- **No unit may use `Requires=glimpsed.service`.** `Wants=` only — the panel, wallpaper and lock are
  specified to survive a dead daemon, and `Requires=` kills them instead.
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
- **Edit the spec before the code, every time, and never without approval.** See Specs come first.

## Finishing

Finishing is a pass over the work, not the moment the last edit compiles. Run it every time, before
saying anything is done.

1. **Formatter and linter clean.** `just fmt`, then `just lint`, and `just verify` when code
   changed. Zero errors and zero warnings — clippy runs with `-D warnings`, so a warning left behind
   is a broken build for whoever runs it next, not a cosmetic note. Silencing one with
   `#[allow(...)]` rather than fixing it needs a reason worth saying out loud.
2. **Re-read the spec against the diff.** Open it; do not go from memory. A default that drifted, an
   exit code that is not the one specified, a flag renamed on one side only — these survive review
   precisely because nobody goes back to the source.
3. **Delete what nothing calls.** Dead functions, unused constants, a type kept "for later", a
   wrapper whose body is a single call, a trait with one implementation. Anything that earns its
   place only in an imagined future has not earned it; add it back in the change that needs it.
4. **Cut the ceremony.** A custom error type carrying no information a message would not, a builder
   for two fields, a helper called once, a test asserting that the standard library works.
5. **Check the docs the change invalidated.** The crate `README.md` first — a stale README is worse
   than none, because it is believed.
6. **Read it as a stranger would.** Would you put this in front of someone whose opinion you value?
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
  responsibility, corrected spec link — all of it lands in the README alongside the code.
- Remove instructions that stop being true rather than adding a caveat beside them. Two rules on the
  same topic produce worse behaviour than one.

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
