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

```go
type CounterState struct {
    Count int
}

type CounterApplet struct {
    sdk.BaseApplet[CounterState]
}

func (a *CounterApplet) Render(context.Context) (sdk.RenderResult, error) {
    tree := sdk.NewColumn([]sdk.TreeNode{
        sdk.NewHero("Counter", fmt.Sprintf("Value: %d", a.State().Count)),
        sdk.NewLabel(fmt.Sprintf("Count = %d", a.State().Count)),
        sdk.NewButton("increment", "Increment"),
    }, 8)
    return sdk.RenderResult{
        Status: []sdk.StatusItem{
            {ID: "counter", Icon: sdk.IconName("view-refresh-symbolic"), Label: fmt.Sprintf("%d", a.State().Count)},
        },
        Tree: &tree,
    }, nil
}
```
