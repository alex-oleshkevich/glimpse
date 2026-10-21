# Stress gate

The last check before production. It runs every glimpse component under load, churn, and a 24-hour soak
on both compositors. It ships only if every scenario passes the thresholds below.

The gate starts after every epic open on 2026-10-21 closes: glimpse-6umks, glimpse-3opwd,
glimpse-8i49g, glimpse-wq94q and glimpse-tz18r. Each of these changes what runs and where: provider
processes, the config watcher, thread counts and the compositor providers. A gate run before them
would measure code that is about to be replaced. The deferred drafts epic glimpse-d3civ does not block
the gate.

## Current state

- **What exists.** Profiling tools are installed and listed on glimpse-8i49g. A memory, thread and CPU
  baseline of the user session is recorded there. glimpse-8i49g.9 turns that baseline into a script.
- **No stress harness exists.** Nothing generates sustained load, and nothing samples components over
  time or decides pass or fail.
- **Fake peers exist**, built for screenshots and popover testing (see Local code). They run once; none
  of them churns.
- **The test VM** (`var/vm`, git-ignored, local to this machine) runs Hyprland only. It has 4 CPUs and
  3 GB, set in `var/vm/run.sh:16`. The README says 6 GB; `run.sh` is what actually runs.
- **This plan adds:**
  - the harness `scripts/stress.py`;
  - churn modes on the existing fakes;
  - niri and input tools in the guest;
  - nine scenarios;
  - one gate report.

## Local code

| What | Where |
| --- | --- |
| MPRIS fakes, three players, one shot | `scripts/mpris-fake-players.py`. `run(plain=…)` at :404; the only flag is `--plain-names` |
| Tray fake | `crates/glimpse-dbus/examples/fake-tray.rs` (`main` at :7), built by `just fake-tray-build` (`justfile:250`) |
| Private buses, mocked system services | `scripts/docs-screenshots.py`: `bus_daemon` :141, `mock` :170, `system_services` :184 |
| Nested niri recipe | `scripts/docs-screenshots.py`: `niri_config` :356, `nested` :400 |
| VM lifecycle | `var/vm/run.sh`, `deploy.sh`, `ssh.sh`, `snap.sh`, `restore.sh`, `stop.sh` (`var/vm/README.md`) |
| Lock trigger | `glimpse-lock lock`. Exit 0 once `LockedHint` is true; exit 4 after 5 s (`crates/glimpse-lock/README.md`, Commands) |
| Lock ignores logind Unlock | `crates/glimpse-lock/src/lifecycle.rs:151-152`, so every unlock is typed |
| Restart policy | `Restart=on-failure` on every unit; `Restart=always` on `glimpse-lock.service` (`data/systemd/*.service`) |
| Panel round trip | `glimpsectl applet list`, answered by the panel process |

## Remote facts

| Fact | Status |
| --- | --- |
| `hyprctl output create headless` adds an output and `hyprctl output remove <name>` drops it | assumed (Hyprland wiki); `.1` measures it |
| `niri msg output <name> off` / `on` emits an output change that glimpse sees as unplug and plug | assumed; `.1` measures it |
| A virtual-keyboard tool reaches the `ext-session-lock` surface. `wtype` uses the virtual-keyboard protocol and the compositor may refuse it while locked; `ydotool` writes uinput and is not subject to that | assumed; `.1` measures it and picks the first one that unlocks |
| `NRestarts` on a user unit counts automatic restarts | assumed (systemd.exec docs); `.1` measures it with one `kill -SEGV` |
| PSS + swap from `smaps_rollup` is the memory number | measured on 2026-10-21 (glimpse-8i49g) |

## Environment

- **Where it runs.** Everything runs inside the VM. Nothing runs on the user's session or screen.
- **How the guest gets the code.**
  - The guest mounts the repo read-only at `/mnt/glimpse`.
  - `deploy.sh <tree>` installs a release build (`GVM_PROFILE=release`), so the gate measures what ships.
  - Debug builds are 3–10× slower and would fail latency thresholds for the wrong reason.
- **Guest additions** in `var/vm/guest/packages.txt`:
  - niri
  - stress-ng
  - sysstat
  - the unlock tool that `.1` picks
  - `uv`, for the Python fakes
- **One provisioned snapshot.** After adding these, take snapshot `stress-base`. Each scenario starts
  from `restore.sh stress-base`, so leftovers from one scenario cannot pollute the next.
- **Compositors.** The VM boots into Hyprland. A second autologin profile, which `.1` writes, boots niri.
  Each scenario runs on both, except where noted.
- **Glimpse runs as shipped.** `systemctl --user start glimpse-session.target` uses the real units and
  the guest's own `~/.config/glimpse`. The VM home is disposable, so the real config path is allowed
  here, and only here.

## Architecture

```
host                                guest (VM, one compositor)
----                                ------------------------------------------------
just stress-gate <scenario>  --ssh-->  /mnt/glimpse/scripts/stress.py <scenario> --out DIR
  restore stress-base                   |- start glimpse-session.target, wait for every unit active
  run.sh, deploy.sh release             |- settle 120 s, sample = before
  copy DIR back to var/stress/          |- start generators (fakes, notify-send, hyprctl/niri, wtype)
                                        |- sample every 10 s while running   (perf-baseline --json)
                                        |- probe panel latency every 1 s     (glimpsectl applet list)
                                        |- stop generators, settle 300 s, sample = after
                                        `- write DIR/report.json, exit 0 pass / 1 fail
```

- **`scripts/stress.py`** is one uv script (same header as `scripts/mpris-fake-players.py`), with one
  subcommand per scenario. It owns sampling, settling, the probe and the verdict. Generators are
  subprocesses it starts and kills.
- **Sampling** calls `scripts/perf-baseline.sh --json`. glimpse-8i49g.9 gains this requirement: it
  runs without `just`, prints JSON with `--json`, and adds an `fds` column. The gate never keeps a
  second reader of `/proc`.
- **`just stress-gate SCENARIO`** is the host recipe. It drives the VM scripts and copies the report to
  `var/stress/<date>-<compositor>-<scenario>/`. The gate result is recorded on the epic as notes, not as
  a repo file.

### Report

```json
{
  "scenario": "s3-notification-flood",
  "compositor": "hyprland",
  "commit": "0921a29d",
  "verdict": "fail",
  "samples": [
    {"t": 0, "phase": "before", "component": "glimpse-panel", "pss_kb": 61234, "swap_kb": 0,
     "anon_kb": 44812, "threads": 41, "fds": 87, "cpu_ms": 1520}
  ],
  "probe_ms": {"idle_p95": 38, "load_p95": 71, "load_max": 240},
  "restarts": {"glimpse-panel.service": 0},
  "coredumps": [],
  "checks": [
    {"id": "P2", "component": "glimpse-notifications", "pass": false,
     "detail": "after 81.2 MB > before 70.1 MB * 1.10 + 4 MB"}
  ]
}
```

## Pass criteria

These apply to every component in every scenario. The `before` and `after` values are the samples from
the flow above.

| Id | Criterion |
| --- | --- |
| P1 | No unexpected death. `NRestarts` is unchanged on every glimpse unit, and `coredumpctl` lists no glimpse binary. S7's deliberate kills are the only exception. |
| P2 | Memory returns. `after` (PSS + swap) ≤ `before` × 1.10 + 4 MB. |
| P3 | Threads and fds return. `after` threads ≤ `before` + 2, and `after` fds ≤ `before` + 4. |
| P4 | CPU settles. Mean CPU over the 300 s settle window is ≤ 0.5 % of one core per component. |
| P5 | The panel answers under load. Probe p95 under load ≤ idle p95 + 100 ms; no probe takes over 1 s; no probe fails. |
| P6 | Scenario check. The post-condition named in that scenario holds. |
| P7 | Soak slope. Over hours 2–24 of S9, a least-squares fit of PSS + swap rises ≤ 0.5 MB per hour per component. |

## Scenarios

| Id | Load | Post-condition (P6) |
| --- | --- | --- |
| S1 MPRIS flood | 40 fake players. Each changes metadata at 5 Hz. One appears or vanishes every 2 s. Runs 10 min. Titles come from the hostile corpus. | Panel players = live bus players. No label exceeds its cap. |
| S2 Tray flood | 40 fake tray items. Icons and menus change at 2 Hz. One registers or unregisters every 2 s. Runs 10 min. | Tray item count = registered items in the watcher. |
| S3 Notification flood | 100 notifications/s for 5 min from 20 app ids. Bodies come from the hostile corpus (64 KB text, markup, RTL, emoji, invalid UTF-8 via a raw D-Bus send). 10 % replace an earlier id. | Store count ≤ its configured cap. Panel and lock badges match `glimpsectl notifications`. |
| S4 Config storm | `config.toml`, `panel.toml`, a `panel.d/` drop-in and the theme sheet are rewritten 10 times/s for 2 min. Every fifth write is invalid TOML or an invalid value. | Each binary ends on the last valid config. The theme alert slot shows the last error, then clears on a valid write. |
| S5 Lock soak | 500 cycles: `glimpse-lock lock`, then type the password, then wait for `LockedHint` false. Every 50th cycle types three wrong passwords first. | Every lock exits 0 within 1 s. A wrong password never unlocks. Zero exit 4. |
| S6 Output churn | 200 cycles of add output, set scale 1 / 1.25 / 2, remove output (Hyprland: headless outputs; niri: output off/on). Repeated with the session locked. | One panel and one wallpaper per output after each cycle. Locked: a `grim` capture of each output shows no pixel of a marker window. |
| S7 Provider churn | Every 30 s for 30 min, `kill -9` one provider in turn, round-robin over every provider unit at gate time. Load from S1 and S3 runs at one tenth rate. | Each consumer renders the provider's state again within 10 s of systemd restarting it. |
| S8 Pressure | S1, S3 and S6 at one third rate, under `stress-ng --cpu 4 --vm 1 --vm-bytes 75%` for 30 min. Then each component runs under `systemd-run --user -p MemoryMax=<its P2 ceiling>`. | Same as S1, S3 and S6. No OOM kill of a glimpse unit. |
| S9 Soak | 24 h of everyday load: a notification every minute, a track change every 3 min, a lock every hour, an output cycle every 2 h, a config write every 30 min. Sampled every 10 min. | P7 holds. P1 holds over the whole day. |

**The hostile corpus** is one file, `scripts/stress-corpus.txt`, written by `.2`. It holds 64 KB lines,
Pango markup, bidi overrides, zero-width joiners, combining-mark stacks, emoji sequences and control
characters. Every generator that sends text takes its strings from it.

## Flows

**A scenario run**, end to end:

1. `just stress-gate s3` restores `stress-base`, boots the VM, deploys release and starts the session.
2. `stress.py s3` waits for every glimpse unit to report `ActiveState=active`. It then settles for 120 s,
   records `before`, and probes idle latency 30 times.
3. It starts the generator processes and samples every 10 s. It probes every second.
4. At the scenario's end it kills the generators, settles for 300 s and records `after`.
5. It checks P1–P6 (P7 for S9), writes `report.json` and exits 0 or 1.
6. The host copies the report to `var/stress/` and stops the VM.

**A failure:**

1. A failed check becomes a bug bead. It carries the report excerpt and blocks the epic.
2. The fix lands in its own worktree.
3. The failing scenario reruns on both compositors.
4. The gate passes only when a single final pass of S1–S9 on both compositors, all at one commit, is green.

## Epic slicing

| Child | Content | Blocked by |
| --- | --- | --- |
| `.1` Harness | Guest packages, niri profile and `stress-base` snapshot. `scripts/stress.py` sampling, settling, probe and verdict. `just stress-gate`. Measures every assumed remote fact. | glimpse-8i49g.9 |
| `.2` Generators | `--count`, `--rate` and `--churn` on `mpris-fake-players.py`; a churn mode on `fake-tray`; the notification flooder (`notify-send` plus a raw `dbus-send` for invalid UTF-8); `scripts/stress-corpus.txt`. | — |
| `.3` S1–S3 bus floods | | `.1`, `.2` |
| `.4` S4 config storm | | `.1`, glimpse-3opwd |
| `.5` S5 lock soak | | `.1` |
| `.6` S6 output churn | | `.1` |
| `.7` S7 provider churn | | `.1`, `.2`, glimpse-wq94q, glimpse-tz18r |
| `.8` S8 pressure | | `.3`, `.6` |
| `.9` S9 soak and gate sign-off | The final green pass of S1–S9 on both compositors at one commit; result noted on the epic. | `.3`–`.8` |

The epic as a whole is blocked by glimpse-6umks, glimpse-3opwd, glimpse-8i49g, glimpse-wq94q and
glimpse-tz18r.

## Struck

- **Running on the user's session or a nested compositor on the host.** Windows open on the user's
  screen and steal focus. The VM shows nothing on the host.
- **A Rust harness crate.** Every generator already exists in Python or as an example binary. A crate
  would add build time and buy no new capability.
- **Fuzzing the D-Bus interfaces (cargo-fuzz).** That is a separate correctness effort, not a load
  gate. The hostile corpus covers the text paths.
- **Valgrind memcheck.** It is slow and noisy with GTK (see glimpse-8i49g). P2 and P7 catch growth;
  heaptrack attributes it once found.
- **Running the gate in CI.** CI minutes are scarce and runners have no compositor. The gate is a manual
  pre-release run.
- **A real GPU and multi-monitor hardware.** The VM's virtio-gpu covers protocol churn. Real panels and
  hotplug on hardware stay on the release checklist.
- **Committing the VM.** `var/vm` holds a 30 GB image and keys. The plan names the guest additions;
  `.1` applies them locally.

## Decisions

The numbers below are the defaults this plan sets; each can be revised before `.1` starts.

1. ~~Which thresholds?~~ P2–P7 as written. They come from the baseline:
   - A headless daemon is 4–6 MB, so 10 % + 4 MB is noise and not growth.
   - 0.5 % of a core is about what an idle Rust daemon uses today. The panel's 2.9 % is glimpse-8i49g.2,
     which closes before the gate.
2. ~~Debug or release?~~ Release. It is what ships, and debug timing fails P5 for the wrong reason.
3. ~~One compositor or both?~~ Both. niri is the primary target, and the VM runs Hyprland today.
4. ~~Soak length?~~ 24 h, sampled every 10 min. That gives 144 points for the P7 fit, where hourly
   sampling would give 23.
5. ~~Where do results live?~~ Reports go in `var/stress/` (scratch); the verdict goes on the epic. A
   report is a session artifact, not a doc.
6. ~~Does glimpse-d3civ block the gate?~~ No. It is a deferred set of reminders. A provider move it later
   produces runs through S7 on the next gate.

## Review

Not reviewed yet. Before `.1` is claimed, a fresh reviewer checks:

- the plan against the tree (every path and line above);
- the decisions;
- whether each P-criterion can fail.

The changes the review causes are recorded here.
