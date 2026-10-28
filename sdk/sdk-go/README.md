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
- `Notify` and `Copy` are
  client-side helpers: they spawn `notify-send` and `wl-copy` from the applet
  process and never touch the wire. `Notify` takes its app name from `Entry.Name`,
  falling back to `Name`, maps an unknown urgency to `normal`, and uses `Entry.Icon`
  when no icon is given. A child that cannot be started writes one line to stderr
  and the applet keeps running.
- Work off the event path (a ticker, a goroutine, a file watch) calls `ctx.Wake()` to schedule
  a rebuild.
- An app embeds `sdk.Applet[YourState]` (with an optional `Initial` value) and runs with
  `sdk.Main(app)`. `State()` returns a copy and `SetState(func(*YourState))` applies a change
  under a lock, persists the whole state as JSON to `state.json` in the state dir (fields
  tagged `json:"-"` are skipped), and wakes a rebuild; it is safe from any goroutine. The
  saved state loads on mount, before the optional one-shot `Start(ctx)` hook, which runs once
  after the host `hello` on the runtime goroutine and starts tickers and watchers there. A
  plain struct with only `Build` keeps working with `sdk.Run` or `sdk.Main`.
- Nothing builds before the host `hello`. A panicking handler exits with code 1 and a tree
  over a protocol limit (or a panicking `Build`) with code 70; a closed socket exits 0.

## Example and tests

Run `examples/kitchen_sink/` against an isolated second panel, never the session bar;
see [live-testing rules](../../docs/live-testing.md). `just sdk-test` tests the SDKs.
