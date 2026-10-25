# glimpse-compositors

The compositor contract crate: the shared model, the D-Bus contract, the provider runtime
and the client mirror.

A compositor-specific provider binary (`glimpse-compositor-niri`, `glimpse-compositor-hyprland`)
implements `Backend` and `serve()`s it on the session bus as `me.aresa.Glimpse.Compositor`;
consumers read `CompositorState` through the `CompositorProvider`/`Handle` mirror; keyboard memory
uses `Handle::keyboard_updates()` to preserve event order. Consumers never see a compositor socket.

This crate touches no Wayland object.

## Contents

| File | Holds |
| --- | --- |
| `model.rs` | `Snapshot` and everything in it, as the provider contract carries it |
| `event.rs` | `Event` and `Resync`, from a backend to the provider runtime |
| `sanitize.rs` | the text caps every backend and the client apply |
| `contract.rs` | bus name, object path, property keys, `Capability`, `Error` |
| `backend.rs` | `Backend`, the trait a provider binary implements |
| `merge.rs` | `apply()`, folding one `Event` into a `Snapshot` |
| `provider.rs` | `run()` and `serve()`, exporting a `Backend` on the session bus |
| `client.rs` | `CompositorState`, `CompositorProvider` and `Handle`, the consumer mirror |

## Rules

- **The provider exits when the compositor event stream ends.**
- **Text from other applications is capped in `sanitize.rs`**, by every backend while decoding
  and again by the client. Text is truncated on a char boundary and stripped of control
  characters and bidi overrides, so no caller can forget to; ellipsizing and markup escaping
  stay the UI's job.
- **`WindowId` and `WorkspaceId` are opaque and scoped to one compositor run.** Never render
  or persist one across sessions.
- **A paused cast is still a live session.** `Cast::active` goes `false` when the consumer
  switches scenes, but the portal grant has not ended — do not read `active` as "is anyone
  capturing right now".
- **Every action names its subject explicitly.** A rename or a window focus carries the id it
  acts on rather than relying on what is focused; only the `step_*` calls lean on focus, by
  design.
- **A title-only change is signalled at most once a second.** The first one goes out at once;
  later ones within the second are held and sent together when it ends. Any other change
  signals immediately and carries the held titles. A spinning terminal title must not wake
  every consumer at the compositor's event rate.
