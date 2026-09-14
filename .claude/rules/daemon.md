---
paths:
  - "crates/glimpse-services/**"
  - "crates/glimpse-notifications/**"
  - "crates/glimpse-weather/**"
  - "crates/glimpse-sunset/**"
---

# Service and provider conventions

## Service handlers

- Handlers run serially on `&mut self`. A handler that can await a backend moves its `Responder`
  into `ctx.spawn`, or one wedged application freezes every other item the service owns.
- No `unwrap()` or `expect()`. A panic takes the service down and cascades `degraded` to dependants.
- No blocking calls: no `std::fs`, no `Command::output()`, no `std::sync::Mutex` held across an
  `.await`. Use `tokio::fs` and `ctx.spawn`.
- A service never retries what its backend already retries, and never reimplements a decision the
  backend makes.
- A long-lived source is declared in `subscriptions`, not started in `start`. The runtime diffs the
  declared set after every input, so dropping a guard is never something a handler has to remember.
  Whatever must force a restart belongs in the `SubKey`; `ctx.spawn` stays for effects that fire
  once.

## Payloads and config

These two rules point in opposite directions on purpose:

- **Config** rejects unknown fields (`#[serde(deny_unknown_fields)]`), so a typo is an error the
  user sees rather than a setting silently ignored.
- **Wire payloads** accept unknown fields, so a newer daemon and an older client survive a version
  skew instead of failing to deserialize.

Payload types derive `PartialEq`. That is the equality gate that stops a service republishing an
identical value.

`S::Config` is bound by `From<&glimpse_config::Config>`, so the projection from the whole document
down to one service's slice is an impl beside the slice — never a method on the service. A service
that reads no configuration uses `type Config = NoConfig;` and writes no impl at all; `()` cannot be
used, because `From<&Config> for ()` is a foreign trait on a foreign type. `S::Config: PartialEq` is
what narrows a reload to the services whose own table moved, and it does so wherever a service runs:
`ServiceRuntime::run` holds the config in force and skips an `Input::Config` equal to it, so the gate
is a property of the framework rather than something each binary has to remember to write.

## Boundaries

- No service crate binds a `wl_` object. Services reach a compositor through `trait Gamma` and
  `glimpse-compositors`, both of which are injected so a headless test can substitute them.
- The dependency order is one-way: `glimpse-services` depends on `glimpse-dbus`, never the reverse.
  A domain type a provider decodes off the bus lives in `glimpse-dbus` beside its decoder;
  everything else lives beside the service that owns it. No GTK in either.
- Nothing depends on a binary crate.
