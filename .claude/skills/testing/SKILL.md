---
name: testing
description: Testing in glimpse — which layer a test belongs to, why GTK tests are one #[ignore]d function per crate, MockBroker for services, splitting pure logic out of GTK- and Ctx-coupled code so it can be tested headlessly, and the mutation check that decides whether an assertion is load-bearing. Use when writing or judging any test in this workspace, when a change has no obvious place to be tested, and before claiming a change is verified. Trigger on the activity, not the wording.
---

# testing

Three tiers, decided by what the code touches. Most defects in this tree have lived in the gap
between them — in code that needed a display or a daemon, so nothing tested it.

**Verified against the tree.** `just` is the only entry point; a recipe that is missing or wrong gets
fixed in the `justfile` rather than worked around with a raw `cargo` invocation.

## Which tier

| The code touches | Tier | Recipe |
| --- | --- | --- |
| Nothing but data | plain `#[test]` | `just test`, `just test-crate <crate>` |
| tokio, a mock broker, a mock bus | `#[tokio::test]` | same |
| GTK widgets | one `#[test] #[ignore]` per crate | `just test-compositor` |
| A live Wayland session | `#[ignore]`, documented as manual | `just test-compositor` |

`just test` must stay green headless, which is why every GTK test is `#[ignore]`d.

## GTK: one test function per crate, not one per property

`gtk4::init()` binds GTK to the calling thread, and cargo runs tests in parallel on many threads. A
second `#[test]` that touches GTK races the first. So each crate has **one** `#[ignore]`d function
that inits once and then runs its assertions in sequence:

```rust
#[test]
#[ignore = "needs a display"]
fn widgets() {
    if gtk4::init().is_err() {
        return;
    }
    register_resources().expect("resources");
    // every widget assertion, in order
}
```

`register_resources()` is not optional: a composite template resolves its resource at class-init, so
constructing one without it fails at the first instantiation. Binaries get this from `main.rs`;
tests do not.

The cost of one function is that a failure stops the rest. That is the accepted trade — the
alternative does not run at all.

## Split pure logic out of coupled code

This is the rule that finds bugs, and every one of these splits was made after a defect hid behind
the coupling:

- Arithmetic behind a `Ctx` — the clamping in an applet's `retime` moved into a free `stepped()`,
  because `Ctx` needs a `Client` and relm4's runtime.
- Bookkeeping behind a widget — scroll accumulation moved into a `Scroll` struct, because reaching
  it through `AppletRuntime` needs GTK.
- Decoding behind a subscription — `payload::<T>` is a free function, so wrong-topic and
  undecodable-payload cases are ordinary tests.

If a property cannot be asserted without a display, ask what part of it is arithmetic and move that
part out. What remains — the call site, the wiring — goes on the GTK test's list, honestly labelled
as uncovered until it gets there.

## Services

`just test-crate glimpse-services` runs every service against mocks with no display, no session bus
and no broker. `MockBroker` is the no-broker case, `Buses::unavailable("...")` the no-bus one. Every
service carries `assert_declarations::<S>()`, because `TOPICS`, `METHODS` and `decode` are not made
to agree at compile time — `const { assert!(...) }` compiles and never fires.

## Never test against the live configuration

`~/.config/glimpse/config.toml` is the user's own. Point every run at a scratch file:

```bash
glimpsed --config "$SCRATCH/config.toml"     # replaces the whole stack, drop-ins included
HOME="$SCRATCH/home" glimpsed                # a fake home, when drop-ins are the thing under test
```

`--config` watches that file's *parent directory*, so redirecting the daemon's log into it makes
every line an event that triggers another read — a closed loop at `DEBOUNCE` that looks exactly like
a watcher retrying. Send the log somewhere else.

`GLIMPSE_THEMES_DIR` and `GLIMPSE_THEME` redirect themes separately, because `theme_dir_for`
resolves through `user_dir()` rather than through the configuration stack.

## The mutation check

**An assertion nobody has broken is not known to test anything.** Before reporting a test as
covering something, break the code it covers and watch it fail:

```bash
cp <file> "$SCRATCH/f.bak"
# invert the condition, delete the guard, discard the remainder
just test-crate <crate> 2>&1 | grep -E "FAILED|left:|right:"
cp "$SCRATCH/f.bak" <file>
```

Mutate the *decision*, not the syntax: drop the early-return guard, discard the carried remainder,
remove the topic check. If nothing fails, the test asserts less than it appears to — that is the
finding, and it is worth more than the test.

This is also how you discover an untested call site. A function can be fully covered while the line
that calls it is not; deleting the call and seeing everything still pass is the only cheap way to
learn that.

## Definition of done

- Every new decision has a test, and every such test has been checked against a deliberately broken
  version of the code it covers.
- Anything that could only be asserted with a display is on the GTK test's list, or is stated as
  uncovered — never implied to be covered.
- `just verify` is clean: `fmt-check`, `check`, `lint` (`-D warnings`, plus units and blueprints) and
  `test`.
- A pre-existing failure is confirmed pre-existing by reading it, not assumed from its name.
