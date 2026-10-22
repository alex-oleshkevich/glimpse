# Compositor providers over D-Bus

| | |
| --- | --- |
| Bead | **glimpse-wq94q** (epic) — tasks are sliced from §11 and carry `Source: docs/plans/compositor-providers.md §N` |
| Status | Approved design (grilling session 2026-10-21, decisions Q1–Q18 in §4). Sliced per §11 into glimpse-wq94q.1–.4 (steps 1–4). |
| Rule | Every claim below is tagged **measured** (run on this machine), **source** (read in upstream or local code, path given) or **documented**. Nothing is assumed. |

## 1. Goal

Today five processes open the compositor socket themselves. After this change:

- `glimpse-compositor-niri` and `glimpse-compositor-hyprland` are the only code that talks to a
  compositor. Each owns `me.aresa.Glimpse.Compositor` and serves interface
  `me.aresa.Glimpse.Compositor1`; systemd starts exactly one of them.
- `glimpse-compositors` becomes the contract crate: model, interface definition, client, and the
  provider runtime both providers share.
- Every consumer (panel, notifications, lock, wallpaper, shot, glimpsectl) reads compositor state
  over D-Bus and follows it.
- The interface is public for **providers**: other developers implement it for their compositor.
  It is documented on `website/developers/compositors.md`. No SDK.

```text
 today                                    after
 panel (2 conns) ──┐                      panel ──────────┐
 lock (2 conns) ───┼──► niri / hypr       lock ───────────┼──► me.aresa.Glimpse.Compositor
 notifications ────┤     socket           notifications ──┤        │ one owner
 shot, wallpaper,  │                      shot, wallpaper,│        ▼
 doctor (one-shot)─┘                      doctor ─────────┘   glimpse-compositor-niri
                                                              | -hyprland | third party
```

## 2. Current state (tree at `e71cb463`)

| What | Where |
| --- | --- |
| `Compositor` enum, `detect_compositor()` reads env, delegates to `Niri`/`Hyprland` | `crates/glimpse-compositors/src/lib.rs:47-170` |
| Model; `Window` deserializes niri JSON directly via `RawWindow` | `crates/glimpse-compositors/src/model.rs:1-236`, `:38-128` |
| Hyprland already has its own wire structs | `crates/glimpse-compositors/src/hyprland/mod.rs:402-570` |
| Normalized `Event`/`Resync` | `crates/glimpse-compositors/src/event.rs:9-50` |
| Capability flags: niri all four, Hyprland `output_power` only | `niri/mod.rs:29-34`, `hyprland/mod.rs:25-30` |
| State merge `apply` lives in the service | `crates/glimpse-services/src/services/compositor.rs:521-608` |
| Keyboard merge `apply_keyboard` + its own backend | `crates/glimpse-services/src/services/keyboard.rs:173, 314-360` |
| Compositor service runs in **three** processes | `glimpse-panel/src/services.rs:104`, `glimpse-notifications/src/services.rs:45`, `glimpse-lock/src/app.rs:797` (keyboard also at `:798`) |
| Live connections to niri: panel 2, lock 2, notifications 1 | **measured** (`ss -xp`, this session) |
| One-shot readers: shot snapshot/screenshot, wallpaper `overview_backdrop`, lock restore notice, doctor | `glimpse-shot/src/adapters/screens.rs:10-19`, `adapters/windows.rs:21-88`, `glimpse-wallpaper/src/app.rs:74-76, 202-207`, `glimpse-lock/src/app.rs:250` + `notify.rs:80-96`, `glimpsectl/src/commands/doctor.rs:23` |
| Layout-code table embedded and installed | `glimpse-compositors/src/keyboard.rs:9`, `data/language-codes.json`, `glimpse-package/Cargo.toml:72,155,227`, `scripts/install.sh:68`, `scripts/package-binary.sh:51` |
| Provider pattern | `glimpse-dbus/src/provider.rs:11-120` (`Exported::start` takes any follow future, `:27-48`) |
| Client pattern: `Follow`/`Snapshots`/`follow_snapshots` (subscribe before read, rebuild on owner change) | `glimpse-dbus/src/clients/follower.rs:57-299`; module private at `clients/mod.rs:107` |
| Private-bus fixture | `glimpse-dbus/src/testing/mod.rs:12-72` |
| Unit/package checks | `justfile:10` (binaries), `scripts/check-units.sh:13-18` |

## 3. Facts

| Fact | Tag |
| --- | --- |
| User manager env has `NIRI_SOCKET`, `WAYLAND_DISPLAY`, `XDG_CURRENT_DESKTOP=glimpse:niri` | measured |
| `niri-session` imports env and unsets `NIRI_SOCKET` only when its `--wait` returns; when niri runs as a user service under an external manager it `exec`s niri and never unsets | source `/usr/bin/niri-session:5-8, 47-53` |
| Hyprland imports `HYPRLAND_INSTANCE_SIGNATURE` into the manager and unsets it on clean exit | source (strings in `/usr/bin/Hyprland` 0.56.2) |
| `ConditionEnvironment=` (systemd 246+) checks the manager env, exact match only; `ExecCondition=` exit 1–254 skips the unit without failing it; systemd here is 262 | documented `man systemd.unit`/`systemd.service`; measured version |
| `test -S` proves a socket file exists, not that anything listens | documented |
| niri `Version` and Hyprland `version` requests exist (used by `--probe`) | measured `niri msg --json version`; source `Hyprland src/ipc/s1/Commands.cpp:1983` |
| Make/model: GDK `GdkMonitor` equals niri IPC byte for byte (incl. trailing space); Hyprland `hyprctl` and `wl_output` both read `m_output->make/model` | measured; source `Hyprland Commands.cpp:277`, `protocols/core/Output.cpp` |
| Description differs: Hyprland IPC sends `m_shortDescription`, `wl_output` sends `m_description`; niri IPC has none | source; `niri/mod.rs:283` |
| niri workspace `is_urgent` = any window on it urgent | source niri `src/layout/workspace.rs` `is_urgent`, `niri-ipc` docs |
| niri casts: `session_id` on every cast, `StopCast` works only for PipeWire, `pw_node_id` is `None` for screencopy | documented `niri-ipc/src/lib.rs` |
| Hyprland screencast events: `screencast>>STATE,TYPE`, `screencastv2>>STATE,TYPE,NAME` with TYPE the word `monitor`, `window` or `region` (NAME = monitor name, or the window title for `window`); wlr-screencopy clients report `region`; a live share emits a start/stop pair every ~0.6 s; no list/stop command exists | measured (Hyprland 0.56.2 VM, event socket during wf-recorder and portal shares) |
| zvariant rejects enums whose variants carry different data; `Option` needs `option-as-array` and still fails as a property | measured (throwaway crate) — moot after Q3 (`a{sv}`) |
| zbus updates the proxy property cache before notifying property streams, under one write lock | source `zbus-5.19.0/src/proxy/mod.rs:445-467` |
| Cost of `Windows` (14 real windows): 3,269 B; emit→3 subscribers decoded p50 0.47 ms, p99 1.0 ms; ~500 µs CPU provider+3 clients, ~80 µs dbus-broker per update | measured (release, real session bus) |
| niri event rate while working: ~0.7 window changes/s, a few focus changes | measured (30 s `event-stream`) |
| Comparable provider RSS: idle 6.7 MB, sunset 8.2 MB, weather 14 MB | measured |

## 4. Decisions

| # | Decision |
| --- | --- |
| Q1 | The interface is a **public provider contract**: third parties implement it; our panel is the client. |
| Q2 | The D-Bus interface is the authority. Documented in `website/developers/compositors.md`. No SDK; the Rust `Backend` trait is internal. |
| Q3 | One D-Bus **property per section**, items as `a{sv}`, **a missing key means absent/unsupported**. Clients ignore unknown keys. No `option-as-array`. |
| Q4/Q7 | Outputs: the compositor is the **only** source (GDK rejected: Hyprland descriptions differ). Display popover loses the **Serial** and **Position** rows. |
| Focus | One `Focus` property `{output, window}`; all `focused` flags are derived client-side. |
| Q5/Q6 | Windows keep `floating` and `order` (both used). Geometry leaves `Windows` and becomes the live method `WindowGeometries()`. `output` is derived from the workspace. |
| Q7 | Workspaces: `{id, index?, name?, output?, active}`; `focused`, `urgent`, window count derived; `active_window_id` dropped (dead). |
| Q8 | Keyboard: providers send what they natively have; **no name→code mapping anywhere**. Badge = user label, else code, else name, raw. `language-codes.json` and `layout_code` are deleted; `[keyboard.labels]` is the user's fix. |
| Q9 | Casts: `{id, active, output?, session?, pw_node?}`; `session` only when `StopScreencast` works for that cast, `pw_node` only when attribution works. Hyprland provider uses `screencastv2`: one cast per session, `output` for monitor/region shares. |
| Q10 | Capabilities: advertised once, read by the service, checked by consumers via `supports()`; the service refuses unsupported commands without a bus call. Only capabilities a consumer branches on: `rename-workspace`, `output-power`, `blank-screens`, `window-screenshot`, `window-geometry`, `overview-backdrop`. |
| Q11 | 12 methods (§6.3). Dropped: workspace focus by index/name (no caller). Provider validates every input (`Refused`). |
| Q12 | `Name` and optional `SessionLockRestore` stay. **No health properties**: a dead compositor is a dead desktop. Fatal error → exit non-zero → `Restart=on-failure`; a failed read mid-session keeps the last state and refetches on the next event. |
| Q13 | Selection: `ConditionEnvironment=` pre-filter + `ExecCondition=<provider> --probe` (a real request). Ours are listed in `glimpse-session.target` `Wants=`; third parties use `WantedBy=glimpse-session.target` + `enable`; `mask` replaces ours. Name clash → exit 4, not restarted. No D-Bus activation file. |
| Q14 | glimpse-shot **degrades only**: no provider behaves like today's unsupported compositor. |
| Q15 | Every long-running consumer **follows** provider state; no one-shot reads at startup (lock restore notice from its mirror; wallpaper reconciles the backdrop surface on capability change). No unit ordering on providers. |
| Q16 | Rollout in four mergeable steps (§11). |
| Q17 | Regression proof: the existing 68 service tests keep their assertions, re-harnessed through fake socket → real provider → private bus → real client → mirror (§10). |
| Q18 | Accepted break: a niri label keyed by the old derived code (`en`) stops matching, because niri sends names only. The `labels` schema doc comment states keys match the compositor's code (Hyprland: `us`) or full name (niri: `English (US)`). The documented form (`config.schema.json:2485`) keeps working. |

## 5. Architecture

### 5.1 Crates

| Crate | Role | Contents |
| --- | --- | --- |
| `glimpse-compositors` (lib) | contract + shared runtime + client | `model` (pure types), `event` (backend→runtime only), `sanitize` (caps, now pub for providers), `contract` (names, keys, capability strings, error names), `backend` (`Backend` trait), `merge` (from `compositor.rs:521-608` + `keyboard.rs:327-360`), `provider` (`serve`), `client` (mirror for consumers) |
| `glimpse-compositor-niri` (bin) | niri provider | the `niri/` module moved in, niri wire structs (`RawWindow`/`RawLayout` become niri-local), `impl Backend`, `main` with `--probe` |
| `glimpse-compositor-hyprland` (bin) | Hyprland provider | the `hyprland/` module moved in, `screencastv2` pairing, `impl Backend`, `main` with `--probe` |

Dependency order stays one way: `glimpse-services → glimpse-compositors → glimpse-dbus`.
`glimpse-compositors` gains `zbus` and `glimpse-dbus`; `rustix` and `dirs` move to the providers.
`glimpse-dbus` re-exports `Follow`, `Snapshots`, `Provider`, `ProviderHandle`, `ProviderError`,
`Changes`, `call` (`clients/mod.rs:107`) and makes `ProviderHandle::proxy` public (`follower.rs:104`).
`.claude/rules/daemon.md` "Boundaries" gets one line rewritten: the compositor contract lives in
`glimpse-compositors`, other provider contracts in `glimpse-dbus`.

### 5.2 Who owns what

| State | Owner | Others |
| --- | --- | --- |
| Merged compositor state | the provider process | mirror via properties |
| `Capabilities`, `Name` | provider, constant for its lifetime | service re-reads on owner change |
| Projections (`WorkspaceInfo` sort, label from description/make/model, urgency roll-up, window count, `focused` flags, pid→id for `focus_app`) | `glimpse-services` | unchanged API for applets |
| Keyboard per-window memory, labels | `glimpse-services` keyboard service | unchanged |
| Window geometry | nobody caches it; asked live | glimpse-shot only |

### 5.3 How the pieces connect

`Backend` (provider crate) → `provider::serve` holds the merged state and applies `Event`s through
`merge` → when a section differs from the last published value, it emits that property's
`PropertiesChanged` (follow future passed to `Exported::start`, `provider.rs:27-48`). On the client,
a proxy with `CacheProperties::Yes` and the existing `follow_snapshots` loop: `read` assembles the
typed state from the property cache, `changes` merges the per-property change streams and assembles
again (safe per §3, zbus cache-before-notify). `decode` re-applies `sanitize` caps (a squatter can own
the name while no provider runs). The mirror `Compositor` service consumes that through `Sub::watch`
and keeps producing today's `CompositorState`. Commands: applet → typed handle → service (capability
check, local guards) → client method → D-Bus method → provider validation → `Backend`.

## 6. The contract

Bus name `me.aresa.Glimpse.Compositor`, object `/me/aresa/Glimpse/Compositor`, interface
`me.aresa.Glimpse.Compositor1`. Keys marked `?` may be absent. Clients ignore unknown keys and
unknown capability strings.

### 6.1 Properties

| Property | Type | Changes | Content |
| --- | --- | --- | --- |
| `Name` | `s` | const | `"niri"`, `"hyprland"`, … |
| `Capabilities` | `as` | const | subset of §6.2 |
| `Outputs` | `aa{sv}` | hotplug, mode, power | `connector s`, `enabled b`, `description s?`, `make s?`, `model s?`, `mode (uuu)?` = width, height, refresh mHz, `logical (iiuud)?` = x, y, width, height, scale |
| `Workspaces` | `aa{sv}` | create, destroy, switch, rename | `id t`, `index y?`, `name s?`, `output s?`, `active b` |
| `Windows` | `aa{sv}` | open, close, retitle, move, urgency, column order | `id t`, `title s?`, `app_id s?`, `pid i?`, `workspace t?`, `floating b`, `urgent b`, `order q?` |
| `Focus` | `a{sv}` | every focus change | `output s?`, `window t?` |
| `Keyboard` | `a{sv}` | layouts, switch | `names as?`, `codes as?`, `current u?` (index into whichever array is present) |
| `Casts` | `aa{sv}` | start, stop, pause | `id t`, `active b`, `output s?`, `session t?`, `pw_node u?` |
| `SessionLockRestore` | `b` | Hyprland config reload | optional property: a provider without the concept does not implement it; absent = unknown = no warning |

Ids are opaque `t`, valid for the provider's lifetime, never rendered or persisted.
Text is capped by the provider and again by the client (titles 512, app ids 128, names 128).

### 6.2 Capabilities

| String | Gates | niri | Hyprland |
| --- | --- | --- | --- |
| `rename-workspace` | `RenameWorkspace`; the workspace-name rename popover (`workspace_name/indicator.rs:83`) | ✓ | ✓ |
| `output-power` | `SetOutputEnabled`; display toggles (`display/indicator.rs:559-560`) | ✓ | ✓ |
| `blank-screens` | `PowerOffMonitors`; power-off button (`display/indicator.rs:561`) | ✓ | ✗ |
| `window-screenshot` | `ScreenshotWindow`; shot strategy (`adapters/windows.rs:53-55`) | ✓ | ✗ |
| `window-geometry` | `WindowGeometries`; shot window mode | ✓ | ✓ |
| `overview-backdrop` | no member; compositor places `glimpse-backdrop` behind its overview; wallpaper backdrop surface (`glimpse-wallpaper/src/app.rs:202-207`) | ✓ | ✗ |

### 6.3 Methods

| Method | Notes |
| --- | --- |
| `FocusWorkspace(t id)` | |
| `StepWorkspace(b forward)` | compositor's own semantics (wrap, per-output scope) |
| `RenameWorkspace(t id, s name)` | `""` clears (middle click, `workspace_name/indicator.rs:49-64`) |
| `FocusWindow(t id)` | pid lookups resolve to an id in the service |
| `StepWindow(b forward)` | |
| `SwitchKeyboardLayout(u index)` | |
| `StepKeyboardLayout(b forward)` | native step, not computed from a possibly stale index |
| `SetOutputEnabled(s connector, b enabled)` | |
| `PowerOffMonitors()` | |
| `StopScreencast(t session)` | |
| `ScreenshotWindow(t id, s path)` | absolute path; returns after the file is fully written |
| `WindowGeometries() → a{ta{sv}}` | asked live; per window `x d`, `y d`, `width d`, `height d`, logical pixels relative to the window's output (today's `Window.rect` meaning). A window whose position the compositor does not report has no entry: niri tiled windows (`glimpse-compositors/README.md:62-65`), which shot captures through `ScreenshotWindow` as today (`adapters/windows.rs:55`) |

### 6.4 Errors

`me.aresa.Glimpse.Compositor1.Error.{Unsupported, Refused, Unavailable, Failed}`, mapping onto
`CommandError::{Unsupported, InvalidArgument, Unavailable, Internal}` so applet notifications read
the same. With no owner ever, the mirror answers `Unsupported` itself (AC-16); a lost owner answers
`Unavailable`.

Provider input validation (`Refused`): unknown workspace/window/output/session id or connector,
names over 128 chars or containing control characters, non-absolute or non-UTF-8 paths. The service
keeps its own guards (last enabled display, `compositor.rs:488-514`) for a fast local answer.

## 7. Internal shapes

- `Backend` trait (internal): `snapshot`, `keyboard_layouts`, `session_lock_restore`, `events`,
  `focus_workspace`, `step_workspace`, `rename_workspace`, `focus_window`, `step_window`,
  `switch_keyboard_layout`, `step_keyboard_layout`, `set_output_enabled`, `power_off_monitors`,
  `stop_screencast`, `screenshot_window`, `window_geometries`, `probe`; constants `NAME`,
  `CAPABILITIES`. Every method takes owned arguments.
- `Capability` enum in `glimpse-compositors` with the six §6.2 variants; parsing drops unknown strings.
- `CompositorState` (client) holds the decoded sections plus `capabilities` and `name`;
  `CompositorHandle::supports(Capability)` in `glimpse-services`.
- Model changes: `KeyboardLayouts.current` becomes `u32`; `Cast` loses `kind` and `target`, gains
  `output`/`session`/`pw_node`; `Window` loses `rect`, `size`, `output`; `Workspace` loses
  `is_focused`, `is_urgent`, `active_window_id`; `Output` loses `serial`, `built_in` (the mirror
  derives it from the connector prefix `eDP-`/`LVDS-`). `Event::WorkspaceActiveWindowChanged`,
  `Event::WorkspaceUrgencyChanged` (urgency is derived from windows, Q7) and the
  `WorkspaceTarget::{Index, Name}` variants are deleted. The provider's merge tracks the focused
  output directly instead of through `Workspace.is_focused` (today `compositor.rs:529-540`).
- `glimpse-services`: `CompositorCapabilities` struct replaced by the capability set;
  `OutputInfo.serial` removed; badge rule per Q8.

## 8. systemd

`data/systemd/glimpse-compositor-niri.service`:

```ini
[Unit]
Description=Glimpse compositor provider (niri)
Documentation=https://github.com/alex-oleshkevich/glimpse
PartOf=glimpse-session.target
After=glimpse-session.target
ConditionEnvironment=NIRI_SOCKET
StartLimitBurst=5
StartLimitIntervalSec=60

[Service]
Type=exec
ExecCondition=/usr/bin/glimpse-compositor-niri --probe
ExecStart=/usr/bin/glimpse-compositor-niri
Restart=on-failure
RestartSec=2
RestartPreventExitStatus=4
NoNewPrivileges=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
LockPersonality=true
RestrictRealtime=true
RestrictSUIDSGID=true
```

The Hyprland unit differs in `Description`, `ConditionEnvironment=HYPRLAND_INSTANCE_SIGNATURE`,
and the binary name. `glimpse-session.target` adds both to `Wants=`. `justfile:10` adds both
binaries. No `dbus-1/services` file. `--probe` connects and sends the version request: exit 0 if the
compositor answers, 1 otherwise. Exit 4 = name already owned.

Third-party template (documented, not shipped): the same unit with
`ConditionEnvironment=<your compositor's variable>`, its own `ExecCondition=<binary> --probe`, and
`[Install] WantedBy=glimpse-session.target`; enable with `systemctl --user enable`, replace one of
ours with `systemctl --user mask`.

## 9. Flows

**Login (niri).** niri imports env → `graphical-session.target` → `glimpse-session.target` wants
both providers → niri unit passes condition and probe, Hyprland unit is skipped → provider connects,
subscribes, reads the first state, owns the name (`Type=exec`: systemd refuses two loaded units with one `BusName=`) → panel, notifications and lock
mirrors see the owner, read all properties, render. Wallpaper and lock may start first: they render
without compositor facts and update when the provider appears (Q15).

**Title change.** niri `WindowOpenedOrChanged` → `merge` → `Windows` differs → one
`PropertiesChanged(Windows)` → three mirrors decode, project, publish only if their slice changed.

**Focus change.** → `Focus` only (~40 B); `Windows` untouched.

**Command.** Click → `CompositorHandle::focus_workspace(Id 3)` → capability check → client →
`FocusWorkspace(3)` → provider validates → `Backend` → reply; state arrives as a property change.
Failure → named error → `CommandError` → notification.

**Provider restart.** Crash → owner gone → mirrors report the provider unavailable, applets keep the
last render → systemd restarts after 2 s → new owner → mirrors re-read everything including
`Capabilities`.

**No provider (panel).** The provider is an enhancement: no owner since the mirror started → mirrors
report "unsupported" with health running, and compositor features are not offered (AC-16). A provider
lost mid-session keeps the last render and marks the mirror degraded (Provider restart).

**Screenshot of a window (niri).** shot reads state and `WindowGeometries()` → `ScreenshotWindow(id,
path)` → provider arms the directory watch, sends the action, replies after `close_write`.

**No provider (shot outside a glimpse session).** Same as today's unsupported compositor: GUI
opens without window picking, `--screen NAME` and single-monitor `--screen` work, `--window`,
`--region`, `--scale logical`, `list` fail with "no compositor provider is running".

**Stale environment.** A crashed niri left `NIRI_SOCKET` and its socket file; the user logs into
Hyprland: niri unit passes `ConditionEnvironment` but `--probe` fails → skipped; Hyprland unit passes
→ correct provider.

## 10. Acceptance criteria

```
AC-1  As a niri user, I want only the niri provider running.
      GIVEN a niri session WHEN glimpse-session.target starts THEN glimpse-compositor-niri is active and
      the Hyprland unit is inactive with a failed condition; `busctl --user status
      me.aresa.Glimpse.Compositor` names the niri provider.
      Verify by: live check, eDP-1 session.

AC-2  As a Hyprland user, I want only the Hyprland provider running.
      GIVEN a Hyprland session WHEN the target starts THEN the mirror of AC-1 holds.
      Verify by: live check in the VM.

AC-3  As a panel user, I want nothing to change.
      GIVEN the scripted inputs of the existing 42 compositor, 14 keyboard and 12 privacy service tests
      WHEN they run through fake socket → real provider → PrivateBus → client → mirror THEN every existing
      assertion passes unchanged (except the fields removed by Q4/Q8, whose assertions are deleted).
      Verify by: the re-harnessed tests (§11 step 2).

AC-4  As a maintainer, I want no compositor socket outside the providers.
      GIVEN the tree WHEN searching THEN NIRI_SOCKET, HYPRLAND_INSTANCE_SIGNATURE and socket code appear only
      under crates/glimpse-compositor-{niri,hyprland}/ and docs; detect_compositor and the Compositor enum
      are gone. Verify by: review grep at step 4.

AC-5  As a panel user, I want an unsupported command to fail fast.
      GIVEN Hyprland WHEN the power-off button would be shown THEN it is hidden; WHEN power_off_monitors is
      called anyway THEN CommandError::Unsupported returns without a bus call.
      Verify by: service test with a fake capability set.

AC-6  As a panel user, I want the panel to survive a provider restart.
      GIVEN a running mirror WHEN the provider's connection drops and a new one owns the name THEN the
      mirror re-reads all properties including Capabilities.
      Verify by: client test on PrivateBus; live `kill` of the provider.

AC-7  As a user, I want a squatter on the name to be harmless.
      GIVEN a fake owner publishing a 1 MB title with bidi overrides WHEN the client decodes THEN the title
      is capped at 512 with hostile characters removed. Verify by: client test.

AC-8  As a user, I want hostile method input refused.
      GIVEN any caller WHEN RenameWorkspace carries control characters or 129 chars, SetOutputEnabled names an
      unknown connector, or ScreenshotWindow gets a relative path THEN Error.Refused and the compositor is
      not called. Verify by: provider tests with a fake backend.

AC-9  As a user, I want the right provider despite stale environment.
      GIVEN a set NIRI_SOCKET whose socket does not answer WHEN the niri unit starts THEN --probe exits 1 and
      the unit is skipped. Verify by: probe test against a fake socket that is closed; live check.

AC-10 As a user, I want no restart loop on a name clash.
      GIVEN one provider owns the name WHEN a second starts THEN it exits 4 once and stays stopped.
      Verify by: provider test (two connections) + RestartPreventExitStatus in the unit.

AC-11 As a niri user, I want the backdrop even if the wallpaper starts first.
      GIVEN the wallpaper running without a provider WHEN the provider appears with overview-backdrop THEN
      the backdrop surface is created. Verify by: wallpaper test with a fake client; live login check.

AC-12 As a Hyprland user, I want the lock-restore warning even if the lock starts first.
      GIVEN the lock running without a provider WHEN the provider appears with SessionLockRestore=false THEN
      the "Lock restore is off" notification is sent once. Verify by: lock test with a fake mirror.

AC-13 As a Hyprland user, I want real screencast rows.
      GIVEN screencastv2>>1,monitor,DP-2 then screencastv2>>0,monitor,DP-2 WHEN decoded THEN Casts holds one cast with
      output DP-2, then none. Verify by: hyprland decoder test.

AC-14 As a glimpse-shot user outside a glimpse session, I want graceful degradation.
      GIVEN no provider owner WHEN `glimpse-shot capture --window` runs THEN it fails with "no compositor
      provider is running"; `--screen NAME` still captures. Verify by: shot headless test with an absent client.

AC-16 As a user on a compositor no provider serves, I want a working desktop without compositor features.
      GIVEN no provider has owned the name since the mirror started WHEN the panel, notifications and lock run
      THEN the Compositor mirror publishes status name "unsupported", no capabilities and no sections, and
      privacy reads not-sharing so notification popups show; the Compositor and Keyboard mirrors' health stays
      running (absence is not a fault), every command answers `CommandError::Unsupported` without a bus call,
      and no notification is sent; when an owner later appears the mirrors follow
      it. A provider lost after it owned the name marks the mirror degraded until a new owner appears.
      `glimpsectl doctor` reports no provider as information, not a failure.
      Verify by: service test on PrivateBus with no owner (health running, sections absent) and with an owner
      that drops (degraded, last state kept); live check with both provider units stopped.

AC-15 As a maintainer, I want CI to enforce units and packaging.
      Verify by: `just verify` (check-units sees both binaries, systemd-analyze verify passes) and
      glimpse-package/tests/packaging.rs listing both units and binaries and no language-codes.json.
```

## 11. Rollout and slicing

Each step merges on its own and leaves the desktop working. One builder per crate.

| Step | Task | Crates | Depends on |
| --- | --- | --- | --- |
| 1 | Contract: model cleanup, `sanitize`, `contract`, `Capability`, `Backend`, `merge`, `provider::serve`, `client`; `glimpse-dbus` re-exports | glimpse-compositors, glimpse-dbus | — |
| 1 | niri provider + unit + `--probe` | glimpse-compositor-niri, data/systemd | contract |
| 1 | Hyprland provider + unit + `--probe` + `screencastv2` | glimpse-compositor-hyprland, data/systemd | contract |
| 1 | Session target `Wants=`, `justfile:10`, packaging; live `busctl` check while old code still runs | data, justfile, glimpse-package | providers |
| 2 | Services become mirrors: `Compositor`, `Keyboard` (Q8 badge rule), `Privacy` (Q9 keys), `supports()`; re-harness the 68 tests (AC-3) | glimpse-services | step 1 |
| 2 | Composition roots build the client: panel, notifications, lock; display popover rows (Q4); rename popover gate (Q10); lock restore notice from the mirror (Q15) | glimpse-panel, glimpse-notifications, glimpse-lock, glimpse-widgets | services |
| 3 | One-shot tools: shot (client, `WindowGeometries`, degrade texts), wallpaper (follow capability), glimpsectl doctor (compositor row, `SessionLockRestore`) | glimpse-shot, glimpse-wallpaper, glimpsectl | step 1 |
| 4 | Delete `detect_compositor`, `Compositor` enum, `layout_code`, `data/language-codes.json` + its package/install lines; docs: `website/developers/compositors.md`, two rows in `website/developers/systemd.md`, `files.md:91` row removed, `labels` schema doc comment, `docs/live-testing.md` compositor paragraph and `.claude/skills/live-testing/SKILL.md` (a panel on a private bus now needs a provider started on that bus, pointed at the nested compositor's socket), `daemon.md` boundary line | all touched | steps 2–3 |

## 12. Rejected

- ~~One typed `State` struct via method + signal~~: forces every implementer to reproduce one
  nested signature exactly; any added field breaks all providers (Q1/Q3).
- ~~`option-as-array`~~: unnecessary once items are `a{sv}`.
- ~~One object per window (`ObjectManager`)~~: measured cost of whole-`Windows` updates is ~0.04 %
  of a core at normal use. Ceiling: a title-spamming app at 60/s costs ~3.5 % of a core
  (arithmetic on the measured per-update cost); then add a per-window signal or a frame cap.
- ~~Output facts from GDK~~: Hyprland descriptions differ (Q4/Q7).
- ~~Name→code mapping (own table or xkb registry)~~: raw strings plus `[keyboard.labels]` (Q8).
- ~~Capabilities for data sections or per command for completeness~~: no consumer branches on them (Q10).
- ~~Health properties~~ (Q12).
- ~~D-Bus activation~~: one name cannot map to two units.
- ~~Socket-file `ExecCondition` and `!NIRI_SOCKET`~~: stale files and variables pick the wrong provider (Q13).
- ~~Shot starts providers on demand~~ (Q14).
- ~~`After=` ordering on providers~~: a hanging provider would delay the lock at login (Q15).
- ~~Debounce timer~~: not needed at measured rates, except Hyprland casts: a live share emits a
  start/stop pair every ~0.6 s, so the Hyprland provider holds a stopped share for 2 s and revives
  it, same id, when the same `(TYPE, NAME)` starts again (user decision 2026-10-22).

## 13. Open questions

None. All closed in §4 (Q1–Q18).

## 14. Review record

- 2026-10-21: drafted with two web-research subagents, then reviewed with the user in a grilling
  session; every decision in §4 was approved by the user. Facts in §3 were measured or read in
  source during that session.
