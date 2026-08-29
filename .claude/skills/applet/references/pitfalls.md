# Pitfalls

Symptom first. Every one of these was a real defect in the tree, not a hypothetical.

## The applet renders nothing and the log says nothing

`indicators()` returned an empty `Vec` and the group hid itself, which is correct behaviour for "no
value yet". Run with `--log debug`: `applet=<name> topic=… event` says data arrived,
`applet=<name> indicators=0 rendered` says the applet chose to show nothing.

If neither line appears, either the subscription never delivered — see the next entry — or the topic
was never declared: `applet=<name> topics=0 started` says `topics()` returned nothing.

## The applet never populates when the panel started before the daemon

`Connection::idle` (`Connection::idle` in `glimpse-ipc`) **fails** every request that arrives while the
daemon is unreachable, with `Unavailable`. A `subscribe` issued in `start` therefore returns `Err`
before `glimpsed` is up.

`Ctx::subscribe` handles this by retrying on every connection-state transition, so an applet — which
only ever *declares* a topic through `topics()` — does not have to. If you add a new source
constructor, it must do the same: giving up on the first `Err` means the applet is dead for the
session, and starting before the daemon is ordinary, since the panel carries
`Wants=glimpsed.service` and never `Requires=`.

There is no unit test for this path; it is checked live by starting the panel first.

## Every applet is destroyed and rebuilt on every theme edit

`App::update` calls `reconcile_panels` on *every* input, including `ThemeChanged`, which fires on
each write to a theme file. `Panel::reconcile_applets` guards against that by comparing the desired
`(zone, name)` sequence against the current one and doing nothing when they match.

The guard is only correct if **every** desired name produces a `Slot`, including one with no
implementation — hence `Slot.handle: Option<AppletHandle>`. Skipping unresolvable names with
`continue` makes the current list permanently shorter than the desired one, so the guard never
holds. The shipped default config names nineteen applets and this tree implements none of them, so
the guard would never hold at all and every theme write would clear and rebuild every zone.

## A scroll gesture fires far too many commands

The `Indicator` emits raw `dx`/`dy`, and a touchpad delivers many small deltas per gesture. The
runtime accumulates per `(indicator, axis)` and drains in whole notches, so a wheel detent is one
notch and ten `0.4` deltas are four. An applet sees `Direction`, never a delta.

If you add a source of pointer input, do not hand raw deltas to `handle`.

## A stock start prints a wall of warnings

Two failures look alike and are not: a zone name that resolves to no `Applet` variant is a typo and is
`warn!`; a kind that resolves but has no arm in `build` is expected and is `debug!`. The default
config names nineteen applets, so collapsing them into one severity means nineteen warnings on an
untouched installation — which teaches people to ignore the line that will later matter.

The same rule applies to anything logged from inside a reconcile: it runs on every input.

## The applet's idea of a value drifts from the daemon's

`ctx.call` is fire-and-forget and discards the reply. An applet that tracks a value it only ever
*sets* — a period, a volume it never reads back — will drift when anything else changes it, because
nothing tells it. Prefer deriving the value from a topic. If a command's reply genuinely is the
answer, that is the case that justifies adding `ctx.ask`.

## A source keeps running after its applet stopped

It should not, and both paths are covered: `Ctx` drops with the runtime, and a panicking applet is
torn down by `ctx.shutdown()`. If you add state that owns a task outside `Ctx`, it is on you to stop
it — the panic path only clears what `Ctx` holds.

## `(init.build)(&ctx)` and other syntax the tree avoids

`init.build` is a field holding a `fn` pointer, and `init.build(&ctx)` parses as a method call.
`applet/runtime.rs` binds it to a local first (`let build = init.build;`) rather than writing the
disambiguating parentheses, because the project forbids the comment that would otherwise be needed
to explain them.
