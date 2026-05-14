# Glimpse Applet Go SDK

Small async-style framework for building Glimpse `exec` applets without touching stdio or raw JSON.

Requires Go 1.24+.

## Install

```sh
go get github.com/alex-oleshkevich/glimpse/sdk/sdk-go
```

The package import path is `github.com/alex-oleshkevich/glimpse/sdk/sdk-go/sdk`.

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
            Icon:  sdk.IconName("view-refresh-symbolic"),
            Label: fmt.Sprintf("%d", state.Count),
        },
    }, nil
}

func (a *CounterApplet) Popover(_ context.Context, state *CounterState) (sdk.Widget, error) {
    return sdk.Column{
        Spacing: 8,
        Children: []sdk.Widget{
            sdk.Hero{Title: "Counter", Subtitle: fmt.Sprintf("Value: %d", state.Count)},
            sdk.Label{Text: fmt.Sprintf("Count = %d", state.Count)},
            sdk.Button{
                CommonProps: sdk.CommonProps{ID: "increment"},
                Label:       "Increment",
                Icon:        "list-add-symbolic",
                Variant:     sdk.ButtonVariantPrimary,
            },
        },
    }, nil
}
```
