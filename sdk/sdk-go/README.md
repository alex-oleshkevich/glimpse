# glimpse-sdk for Go

Write a glimpse panel applet in Go. The applet is a Go program: the panel starts it as a
systemd user unit, and the SDK speaks the panel's NDJSON protocol on the unix socket named by
`GLIMPSE_APPLET_SOCKET`, which the panel sets (without it the applet exits with code 1). The wire format is
specified in [PROTOCOL.md](../sdk-ts/PROTOCOL.md) and is shared with the other SDKs. The SDK
uses only the standard library.

```go
package main

import sdk "github.com/alex-oleshkevich/glimpse/sdk/sdk-go/sdk"

type app struct{}

func (app) Build(ctx *sdk.Context) []sdk.Widget {
	return []sdk.Widget{
		sdk.Indicator(sdk.IndicatorProps{Icon: "face-smile-symbolic", Tooltip: "Hello"}, "Hello"),
		sdk.Popover(sdk.PopoverProps{Icon: "face-smile-symbolic", Title: "Hello"},
			sdk.Section(sdk.SectionProps{Title: "Hello"},
				sdk.Row(sdk.RowProps{Title: "It works"}))),
	}
}

func main() { sdk.Run(app{}) }
```

## Model

- `Build(ctx)` returns a fresh widget tree on every rebuild. Widgets are plain values: typed
  props (`IndicatorProps`, `NoticeProps`, …) flow down, and handler fields (`OnToggle`,
  `OnChange`, …) flow up. Zero-valued fields are omitted from the wire; pointer fields keep an
  explicit zero distinct from absence, and `sdk.Ptr` builds one inline.
- Handlers mutate state; the runtime rebuilds after every host message and diffs the new tree
  against the mounted one, emitting one protocol commit per rebuild.
- A widget keeps its identity across rebuilds when its kind and `key` match in the same parent.
  Keyed rows that reorder emit `move`, and their controlled-edit sequence follows them. A kind
  change at a slot unmounts and remounts. A move across parents is a remove plus an insert.
- Controlled props (`value`, `active`, `open`, `selected`) always come from state. An event's
  `seq` is copied onto the confirming `set`; an event older than the last applied `seq` is
  dropped; a rejected edit still confirms, so the panel restores the state value.
- `Context` carries `Name`, `StateDir`, `Options`, `Placement`, `PopoverOpen`, and the
  `Mpris` and `Compositor` service snapshots, plus the host requests (`Notify`, `Copy`,
  `OpenURI`, `Launch`, `ClosePopover`, MPRIS control/seek/set-position, workspace and window
  commands). `PositionNow` advances a playing track's position.
- Work off the event path (a ticker, a goroutine, a file watch) calls `ctx.Wake()` to schedule
  a rebuild.
- Nothing builds before the host `hello`. A panicking handler exits with code 1 and a tree
  over a protocol limit (or a panicking `Build`) with code 70; a closed socket exits 0.

## Example and tests

`examples/kitchen_sink/` exercises every element kind, every handler, both services, host
requests, persistence, a one-second ticker, and keyed reordering. Run it against an isolated
second panel — never the session bar — as described in `docs/live-testing.md`.

```
go test ./...
```
