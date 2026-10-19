---
name: testing
description: Testing in glimpse — which layer a test belongs to, where test code lives (a sibling tests.rs or tests/*.rs, never inline), why GTK tests are #[ignore]d and run one per process, the manual test fixtures in scripts/, splitting pure logic out of GTK- and Ctx-coupled code so it can be tested headlessly, and the mutation check that decides whether an assertion is load-bearing. Use when writing or judging any test in this workspace, when a change has no obvious place to be tested, and before claiming a change is verified. Trigger on the activity, not the wording.
---

# testing

Three tiers, decided by what the code touches. Most defects in this tree have lived in the gap
between them — in code that needed a display or a daemon, so nothing tested it.

**Verified against the tree.** `just` is the only entry point; a recipe that is missing or wrong gets
fixed in the `justfile` rather than worked around with a raw `cargo` invocation.

**Tests never share a file with the code they test.** A module's tests go in a sibling `tests.rs`, or
a `tests/` directory of files, declared with `#[cfg(test)] mod tests;`; a crate's integration tests
go in its `tests/*.rs`. An inline `mod tests { … }` is a defect wherever it appears.

## Which tier

| The code touches                            | Tier                              | Recipe                                 |
| ------------------------------------------- | --------------------------------- | -------------------------------------- |
| Nothing but data                            | plain `#[test]`                   | `just test`, `just test-crate <crate>` |
| tokio, a fake dependency handle, a mock bus | `#[tokio::test]`                  | same                                   |
| GTK widgets                                 | `#[test] #[ignore]`, one process each | `just test-crate-compositor <crate>` |
| A live Wayland session                      | `#[ignore]`, documented as manual | `just test-compositor`                 |

`just test` must stay green headless, which is why every GTK test is `#[ignore]`d.

## GTK: one process per test

`gtk4::init()` binds GTK to the calling thread, and cargo runs tests in parallel on many threads, so
two GTK tests in one process race. Every GTK test is `#[ignore]`d and runs alone:
`just test-compositor` and `just test-crate-compositor <crate> [FILTER]` start one process per
ignored test, and `just test-one <crate> <path>` runs one. A crate has as many GTK tests as it has
properties to check; `cargo test -- --ignored` runs them together and proves nothing.

```rust
#[test]
#[ignore = "needs a display"]
fn the_row_shows_its_subtitle() {
    gtk4::init().expect("display");
    register_resources().expect("resources");
}
```

`register_resources()` is not optional: a composite template resolves its resource at class-init, so
constructing one without it fails at the first instantiation. Binaries get this from `main.rs`;
tests do not.

## Split pure logic out of coupled code

This is the rule that finds bugs, and every one of these splits was made after a defect hid behind
the coupling:

- Arithmetic behind a `Ctx` — the clamping in an applet's `retime` moved into a free `stepped()`,
  because `Ctx` needs a `Client` and relm4's runtime.
- Bookkeeping behind a widget — scroll accumulation moved into a `Scroll` struct, because reaching
  it through `AppletRuntime` needs GTK.

If a property cannot be asserted without a display, ask what part of it is arithmetic and move that
part out. What remains — the call site, the wiring — gets an `#[ignore]`d GTK test, or is stated as
uncovered until it does.

## Services

`just test-crate glimpse-services` runs every service headlessly, with no display and no live bus.
`Buses::unavailable("...")` is the no-bus case, and a dependency is supplied as a fake typed handle
rather than by starting its producer. A service's interface is its own `State`, `Command` and
`Event` types, so the compiler checks that both ends agree and no assertion stands in for it.

## Never test against the live configuration

`~/.config/glimpse/config.toml` is the user's own. Point every run at a scratch file:

```bash
glimpse-panel --config "$SCRATCH/config.toml"  # replaces the whole stack, drop-ins included
HOME="$SCRATCH/home" glimpse-panel            # a fake home, when drop-ins are the thing under test
```

`--config` watches that file's _parent directory_, so redirecting the daemon's log into it makes
every line an event that triggers another read — a closed loop at `DEBOUNCE` that looks exactly like
a watcher retrying. Send the log somewhere else.

`GLIMPSE_THEMES_DIR` and `GLIMPSE_THEME` redirect themes separately, because stylesheet resolution
runs through `user_dir()` rather than through the configuration stack.

## The mutation check

**An assertion nobody has broken is not known to test anything.** Before reporting a test as
covering something, break the code it covers and watch it fail:

```bash
cp <file> "$SCRATCH/f.bak"
# invert the condition, delete the guard, discard the remainder
just test-crate <crate> 2>&1 | grep -E "FAILED|left:|right:"
cp "$SCRATCH/f.bak" <file>
```

Mutate the _decision_, not the syntax: drop the early-return guard, discard the carried remainder,
remove the filter. If nothing fails, the test asserts less than it appears to — that is the
finding, and it is worth more than the test.

This is also how you discover an untested call site. A function can be fully covered while the line
that calls it is not; deleting the call and seeing everything still pass is the only cheap way to
learn that.

## What only a human can check

Some behaviour has no headless assertion: a pointer press claimed by one widget rather than another,
a hover highlight, a dialog that maps, a compositor placing a surface, a notification appearing. The
rule is not "test it anyway" — a test that cannot fail is worse than none, because it reads as
coverage. The rule is to **hand the check to the person who can run it**.

**A change that can only be confirmed on a live session ends with a numbered manual test list.** Each
step says what to do, what to look for, and what would count as a failure. Report it in the reply;
it does not go in a crate README, which AGENTS.md keeps free of what was tested.

```
1. Restart the panel and open the bluetooth popover.
   Expect: `gdbus` shows Discoverable: true, and false within a second of closing it.
   Fails if: the flag stays set after the popover is gone.
2. Click the body of a switch row, then the knob itself.
   Expect: one confirmation dialog each time.
   Fails if: the knob produces two, or refuses to move.
```

Write the step you actually cannot automate, not the one you did not get to. "Check it looks right"
is not a step; "the knob must flip and exactly one dialog must appear" is.

## Manual fixtures in `scripts/`

Most scripts in `scripts/` that no recipe calls are fixtures for exactly these manual checks: fake
MPRIS players, a fake removable drive, a mock CUPS, privacy-indicator triggers, network profiles,
a lock rescue. The catalog, with usage and traps, is `references/scripts.md`.

- Reach for an existing fixture before writing one. A new fixture goes in `scripts/` with a usage
  header and an entry in the catalog.
- No recipe calling a script does not make it dead; never delete one on that ground.
- A fixture that writes configuration gets scratch paths, the same as the panel.
- A fixture that needs sudo is a step for the user in the manual list, never run by an agent.
- Arm `just net-guard` before anything that takes a network connection down.

## Live test every code change

Every change to code that runs (not docs) is live-tested before it is called done, by a cheap
capable model in a fresh session that did not write it: a sonnet subagent, or pi with DeepSeek
(`pi -p --no-session -ne --no-mcp -ns -np --model openrouter/deepseek/deepseek-v4-flash-latest`).
It follows the `live-testing` skill and runs everything on the laptop screen eDP-1, never DP-2:
the nested niri window and any test panel move there right after they start. Its brief names the
acceptance criteria to drive, and its report says what it saw, not what it assumed.

## Definition of done

- Every code change passed a live test on eDP-1 when it is available otherwise on any other secondary display. Use primary when it is the only one available display.
- Every new decision has a test, and every such test has been checked against a deliberately broken
  version of the code it covers.
- Anything that could only be asserted with a display has an `#[ignore]`d GTK test, or is stated
  as uncovered — never implied to be covered.
- Every test sits in a `tests.rs` or `tests/*.rs`, none inline.
- Anything no test can reach at all leaves the change as a numbered manual list in the reply, with an
  expectation and a failure condition per step.
- `just verify` is clean: `fmt-check`, `check`, `lint` (`-D warnings`, plus units and blueprints) and
  `test`.
- A pre-existing failure is confirmed pre-existing by reading it, not assumed from its name.
