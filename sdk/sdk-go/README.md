# Glimpse Applet Go SDK

Small async-style framework for building Glimpse `exec` applets without touching stdio or raw JSON.

Requires Go 1.24+.

## Install

```sh
go get github.com/alex-oleshkevich/glimpse/sdk/sdk-go
```

The package import path is `github.com/alex-oleshkevich/glimpse/sdk/sdk-go/sdk`.

## Develop

Create and live-run a Go applet project with the Glimpse tooling:

```sh
glimpse-shell applets new counter --lang go
cd counter
glimpse-shell applets dev
```

Read `docs/custom-applets/tooling.md` for project layout, `applet.toml`, dev applets, local linking, distribution, and diagnostics.

## Goals

- typed protocol models
- typed widget builders
- generic stateful applet API: `Status(ctx, *state)`, `Popover(ctx, *state)`, plus event handlers
- state-driven rendering via `SetState(func(*State))`
- struct-literal widget composition (Flutter-style)

## Example

```go
type CounterState struct {
    Count int
}

type CounterApplet struct {
    sdk.BaseApplet[CounterState]
}

func (a *CounterApplet) Status(_ context.Context, state *CounterState) ([]sdk.StatusItem, error) {
    return []sdk.StatusItem{
        {
            ID:    "counter",
            Icon:  "view-refresh-symbolic",
            Label: fmt.Sprintf("%d", state.Count),
        },
    }, nil
}

func (a *CounterApplet) Popover(_ context.Context, state *CounterState) (sdk.Widget, error) {
    return sdk.Column{
        Spacing: 8,
        Children: []sdk.Widget{
            sdk.Hero{Title: "Counter", Subtitle: fmt.Sprintf("Value: %d", state.Count)},
            sdk.Label{Label: fmt.Sprintf("Count = %d", state.Count)},
            sdk.Tile{
                Primary:     "Increment",
                LeftIcon:    "list-add-symbolic",
                OnClick: func(sdk.CallbackEvent) error {
                    a.SetState(func(state *CounterState) {
                        state.Count++
                    })
                    return nil
                },
            },
        },
    }, nil
}
```

## IPC client

Talk to a running Glimpse daemon: subscribe to event channels and dispatch
actions. `IPC(service)` only resolves the socket path — the connection is
opened lazily.

```go
ctx := context.Background()
sub := sdk.IPC("shell") // "shell" | "wallpaper" | "idle" | "lock"

// Fire an action; awaits the ack, errors if the server rejects it.
ack, err := sub.Dispatch(ctx, "open_uri", map[string]string{
    "uri": "https://example.com",
})

// Stream events until the socket closes.
events, err := sub.Listen(ctx, "audio.*")
for ev := range events {
    fmt.Println(ev.Name, ev.Fields)
}
_ = sub.Err() // why the stream ended (nil = clean EOF)
```
