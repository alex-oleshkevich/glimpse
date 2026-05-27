package main

import (
	"context"
	"fmt"

	sdk "github.com/alex-oleshkevich/glimpse/sdk/sdk-go/sdk"
)

type counterState struct {
	Count int
}

type counterApplet struct {
	sdk.BaseApplet[counterState]
}

func newCounterApplet() *counterApplet {
	return &counterApplet{
		BaseApplet: sdk.NewBaseApplet(counterState{}),
	}
}

func (a *counterApplet) Status(_ context.Context, state *counterState) ([]sdk.StatusItem, error) {
	return []sdk.StatusItem{
		{
			ID:    "counter",
			Icon:  "view-refresh-symbolic",
			Label: fmt.Sprintf("%d", state.Count),
		},
	}, nil
}

func (a *counterApplet) Popover(_ context.Context, state *counterState) (sdk.Widget, error) {
	return sdk.Column{
		Spacing: 8,
		Children: []sdk.Widget{
			sdk.Hero{Title: "__NAME__", Subtitle: fmt.Sprintf("Value: %d", state.Count)},
			sdk.Label{Label: fmt.Sprintf("Count = %d", state.Count)},
			sdk.Tile{
				Primary:  "Increment",
				LeftIcon: "list-add-symbolic",
				OnClick: func(sdk.CallbackEvent) error {
					a.SetState(func(state *counterState) {
						state.Count++
					})
					return nil
				},
			},
		},
	}, nil
}

func main() {
	if err := sdk.Run[counterState](context.Background(), newCounterApplet()); err != nil {
		panic(err)
	}
}
