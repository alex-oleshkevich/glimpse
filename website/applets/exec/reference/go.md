# Go SDK

The module `github.com/alex-oleshkevich/glimpse/sdk/sdk-go`, package `sdk`. It uses only the
standard library. Elements and their props are in [Elements](/applets/exec/reference/elements);
this page covers how a Go applet is put together.

```go
package main

import sdk "github.com/alex-oleshkevich/glimpse/sdk/sdk-go/sdk"

type state struct {
	Done bool `json:"done"`
}

type app struct {
	sdk.Applet[state]
}

func (a *app) Build(ctx *sdk.Context) []sdk.Widget {
	done := a.State().Done
	return []sdk.Widget{
		sdk.Indicator(sdk.IndicatorProps{Icon: "view-list-symbolic", Text: "Todo"}),
		sdk.Popover(sdk.PopoverProps{Title: "Todo"},
			sdk.Section(sdk.SectionProps{},
				sdk.ToggleRow(sdk.ToggleRowProps{
					Title:  "Buy groceries",
					Active: done,
					OnToggle: func(active bool) {
						a.SetState(func(s *state) { s.Done = active })
					},
				}))),
	}
}

func main() {
	sdk.Main(&app{})
}
```

## The applet

Embed `sdk.Applet[S]` in your app struct. `S` is your state; its exported fields are saved as JSON.

| Member | What it does |
| --- | --- |
| `Build(ctx *sdk.Context) []sdk.Widget` | Returns the whole widget tree. Called after every change; keep it free of side effects. |
| `Initial S` | The state before anything is saved. |
| `State() S` | A copy of the current state. |
| `SetState(func(*S))` | Changes the state, writes it to `state.json` in the state directory, and rebuilds. Safe from any goroutine. Fields tagged `json:"-"` are not saved. |
| `Start(ctx *sdk.Context)` | Optional. Runs once after the panel's `hello`, after the saved state loads. Start tickers and goroutines here. |
| `sdk.Main(app)` | Runs the applet. `sdk.Run` is the same function. |

Each element is a function taking a props struct and children: `sdk.ToggleRow(sdk.ToggleRowProps{...})`.
Props are struct fields (`Title`, `ClassName`, `OnToggle`). A zero value is left out; use
`sdk.Ptr(0)` for a pointer field that must send an explicit zero. `Key` keeps a row's identity when a
list reorders.

## The context

```go
type Context struct {
	Name        string         // this instance's name in the config
	StateDir    string         // a directory for your files, mode 0700
	Options     map[string]any // the config's options; updates without a restart
	Placement   map[string]any // output, position, orientation, zone, size of the bar
	PopoverOpen bool           // whether the popover is showing
	Entry       *AppletEntry   // the desktop entry, or nil
}

type AppletEntry struct {
	ID   string
	Name string
	Icon string
}

type Notification struct {
	Summary string
	Body    string
	Icon    string
	Urgency Urgency
}

func (c *Context) Notify(n Notification) // runs notify-send
func (c *Context) Copy(text string)      // runs wl-copy
func (c *Context) OpenURI(uri string)
func (c *Context) Launch(id string)
func (c *Context) ClosePopover()
func (c *Context) Wake() // asks for a rebuild after work outside a handler
```

`OpenURI`, `Launch` and `ClosePopover` ask the panel; see
[Notify, copy, open links and launch apps](/applets/exec/host).

## Exit codes

| Code | Cause |
| --- | --- |
| 0 | The panel closed the socket |
| 1 | A handler panicked, or `GLIMPSE_APPLET_SOCKET` is not set |
| 70 | The tree broke a protocol limit, or `Build` panicked |
