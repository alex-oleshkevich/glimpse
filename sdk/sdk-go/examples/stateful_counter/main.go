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

func (a *counterApplet) OnStart(context.Context) error               { return nil }
func (a *counterApplet) OnInit(context.Context, sdk.InitEvent) error { return nil }

func (a *counterApplet) OnCallback(_ context.Context, event sdk.CallbackEvent) error {
	if click, ok := event.(sdk.ClickEvent); ok && click.ID == "increment" {
		a.SetState(func(state *counterState) {
			state.Count++
		})
	}
	return nil
}

func (a *counterApplet) Render(context.Context) (sdk.RenderResult, error) {
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
				sdk.Button{CommonProps: sdk.CommonProps{ID: "increment"}, Label: "Increment"},
			},
		},
	}, nil
}

func main() {
	if err := sdk.Run[counterState](context.Background(), newCounterApplet()); err != nil {
		panic(err)
	}
}
