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
- generic stateful applet API
- state-driven rendering via `SetState(...)`
- single `Render()` method returning all panel state

## Example

Widget trees are plain struct literals — a Flutter-style composition.
Every widget implements the `sdk.Widget` interface, so containers
can hold heterogeneous children via `[]sdk.Widget`.

```go
type CounterState struct {
    Count int
}

type CounterApplet struct {
    sdk.BaseApplet[CounterState]
}

func (a *CounterApplet) Render(context.Context) (sdk.RenderResult, error) {
    count := a.State().Count
    return sdk.RenderResult{
        Status: []sdk.StatusItem{
            {
                ID:    "counter",
                Icon:  sdk.IconName("view-refresh-symbolic"),
                Label: fmt.Sprintf("%d", count),
            },
        },
        Tree: sdk.Column{
            Spacing: 8,
            Children: []sdk.Widget{
                sdk.Hero{Title: "Counter", Subtitle: fmt.Sprintf("Value: %d", count)},
                sdk.Label{Text: fmt.Sprintf("Count = %d", count)},
                sdk.Button{
                    CommonProps: sdk.CommonProps{ID: "increment"},
                    Label:       "Increment",
                },
            },
        },
    }, nil
}
```
