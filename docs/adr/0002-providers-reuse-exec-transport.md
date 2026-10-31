# External providers reuse the exec-applet transport and element tree

External providers speak a provider protocol carried over the same transport as exec applets: one Unix socket per provider, newline-delimited JSON with the same line and rate caps, a transient systemd user unit per provider, and discovery through desktop entries that declare `Implements=me.aresa.Glimpse.Provider1`. Results are flat items; pushed pages are element trees rendered by the same reconciler as exec applet popovers. Applet authors already know this transport and the four SDKs already implement it, so a provider is a small addition to each SDK rather than a second stack, and the supervision, kill-only-the-offender and hostile-text rules come for free.

## Considered Options

- A D-Bus interface per provider, as GNOME search providers and KRunner runners use: activation would be free, but every SDK would need a second transport, and pages would need another channel.
- JSON over the provider's stdin/stdout, spawned by the launcher: simplest for scripts, but the launcher would own environment, crash handling and supervision that systemd already provides for exec applets.
