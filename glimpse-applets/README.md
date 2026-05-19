# Glimpse Applets

Rust applet sources live under `glimpse-applets/<name>/src`.

Each applet directory must contain:

```text
glimpse-applets/<name>/
  applet.toml
  Cargo.toml
  Cargo.lock
  src/
    main.rs
```

The binary package build runs `scripts/build-glimpse-applets.sh`, which builds
each applet with `cargo build --release --locked --manifest-path
glimpse-applets/<name>/Cargo.toml` and installs `applet.toml` plus the
built binary into `/usr/share/glimpse/applets/<name>/`.

The Rust package/binary name must match the applet directory name.
