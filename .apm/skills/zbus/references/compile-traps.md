# Compile Traps — Keyed by Error Text

When a zbus call fails to compile, search this file for the error text before rewriting the call.
Every entry below was reproduced against zbus 5.16 with `default-features = false, features =
["tokio"]`; the messages are copied from `cargo check` output, not paraphrased.

Remember that `just lint` runs clippy with warnings as errors, so a deprecation is a build failure
here even though it compiles elsewhere.

---

### `Either "async-io" (default) or "tokio" must be enabled.`

A `compile_error!` from `zbus/src/lib.rs`. You set `default-features = false` and forgot the
runtime feature. Neither is enabled, so there is no reactor.

```toml
zbus = { version = "5", default-features = false, features = ["tokio"] }
```

---

### `error[E0061]: this function takes 3 arguments but 1 argument was supplied`

Pointing at `SomeProxy::new(&conn)`, with the note *"this error originates in the attribute macro
`proxy`"*.

The trait declared neither `default_service` nor `default_path`, and did not set `assume_defaults`.
Without defaults the macro generates `new(conn, destination, path)` instead of `new(conn)`.

Note that the `assume_defaults` doc comment claims the macro emits a warning in this case. **It does
not** — as of 5.16 the value simply defaults to `false` and generation proceeds silently. The
three-argument `new` is the only signal you get.

Fix by declaring the defaults, which is what you want anyway:

```rust
#[proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
```

Or, for a trait whose objects live at many paths, keep it defaultless and use
`SomeProxy::builder(&conn).path(p)?.build().await?`.

---

### `error[E0425]: cannot find type 'SomeProxyBlocking' in this scope`

Working as designed. The `blocking-api` feature is off in this workspace, so no blocking proxy type
is generated — `gen_blocking` on the trait is ignored. There is no blocking D-Bus API to reach for
inside a service handler, which is the point.

Rewrite the call site as async. If it genuinely cannot be async, it belongs in `ctx.spawn`, not in a
blocking proxy.

---

### `error[E0433]: cannot find 'dbus_interface' in 'zbus'` (or `dbus_proxy`)

You are following an example written for zbus 2.x or 3.x. The macros were renamed to `interface`
and `proxy` in zbus 4, where the old names survived as deprecated aliases; zbus 5 removed them.

```rust
#[zbus::interface(name = "org.example.Thing")]   // not dbus_interface
#[zbus::proxy(interface = "org.example.Thing")]  // not dbus_proxy
```

Treat the whole example as suspect — anything that old will have other zbus 5 breakage in it.

---

### `warning: use of deprecated type alias 'zbus::object_server::SignalContext': Please use 'SignalEmitter' instead.`

`SignalContext` still exists in zbus 5 as `pub type SignalContext<'s> = SignalEmitter<'s>`, so this
is a warning rather than an error — until `just lint` turns it into one.

Rename the type and the argument attribute together; the attribute is `#[zbus(signal_emitter)]`.

---

### `error: custom attribute panicked` / `help: message: assertion failed: name.starts_with("set_")`

The macro panics rather than producing a useful diagnostic, and points at the whole `#[interface]`
attribute instead of the offending method.

A property setter must be named `set_<property>`. A trailing `_set` does not work:

```rust
#[zbus(property)]
fn volume(&self) -> u32 { self.v }          // getter → "Volume"

#[zbus(property)]
fn set_volume(&mut self, v: u32) { … }      // setter → "Volume". Not `volume_set`.
```

Any `custom attribute panicked` from `#[interface]` is a malformed method signature. Read the
`help: message:` line — it is the actual assertion.

---

### `error[E0308]: mismatched types`, with one arm pointing into `zbus-5.x/src/fdo/error.rs`

Raised at the `#[interface]` attribute, from a method marked `#[zbus(property)]`.

A property method may return a bare `T` or a `zbus::fdo::Result<T>`. It may not return
`Result<T, E>` for any other `E` — there is no conversion, because the wire needs a D-Bus error
name.

```rust
#[zbus(property)]
fn thing(&self) -> zbus::fdo::Result<u32> { … }
```

Map your domain error at the boundary, or derive `zbus::DBusError` on it so it has a name.

---

### `error: unknown attribute 'struct_return'`

Removed after zbus 1.x. To return several values, return a tuple and name the members:

```rust
#[zbus(out_args("answer", "question"))]
fn meaning_of_life(&self) -> zbus::fdo::Result<(i32, String)> { … }
```

To return one struct, return a tuple containing it — a bare struct return is not the same shape on
the wire.

---

### `Builder::build()` returns `Err`, or `Error::InterfaceExists(iface, path)`

You declared `Peer`, `Introspectable` or `Properties` yourself. `serve_at` and `ObjectServer::at`
add all three on your behalf; declaring one is a conflict, not an override.

`InterfaceExists` also fires when the same interface type is exported twice at one path. Note the
asymmetry: `Builder::serve_at` replaces silently, `ObjectServer::at` returns `Ok(false)`.

---

### A signature error at runtime, not compile time

zvariant checks signatures when the message is deserialized, so a wrong Rust type for a D-Bus type
compiles and then fails on the first read. The ones that actually bite:

| Reading | Declared | Correct |
| --- | --- | --- |
| NM `Ssid` (`ay`) | `String` | `Vec<u8>` — and it need not be UTF-8 |
| NM `Strength` (`y`) | `u32` | `u8` |
| NM `LastSeen` (`i`) | `u32` | `i32` |
| MPRIS `mpris:length` (`x`) | `u64` | `i64`, microseconds |
| any `o` | `String` | `OwnedObjectPath` |

`busctl introspect` prints the true signature next to every member. Check it there rather than
inferring from the name — see `references/interfaces.md`, which records the ones glimpse uses.
