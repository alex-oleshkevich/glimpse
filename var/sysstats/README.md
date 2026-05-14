# Sysstats Custom Applet

Go-based custom applet for Glimpse panel system stats.

## Build

```bash
go build ./...
```

## Config

Lookup order:

1. `GLIMPSE_SYSSTATS_CONFIG`
2. `./sysstats.toml`
3. `$XDG_CONFIG_HOME/glimpse/sysstats.toml`

## Panel Example

```toml
[applets.sysstats]
extends = "exec"
command = ["var/sysstats/sysstats-applet"]
```
