# Exec SDK Demo Applets

These demo applets are the maintained smoke surface for the exec applet SDKs.
Each language implements the same realistic workstation status applet and uses
the full public component set plus click, toggle, change, input, scroll, and
popover lifecycle callbacks.

## Python

```bash
PYTHONPATH=sdk/sdk-py python3 var/demo/py/main.py
```

## TypeScript

```bash
cd var/demo/ts
npm install
npm run build
npm start
```

## Rust

```bash
cargo run --manifest-path var/demo/rs/Cargo.toml
```

## Go

```bash
cd var/demo/go
go run .
```
