package main

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	sdk "github.com/alex-oleshkevich/glimpse/sdk/sdk-go/sdk"
)

type item struct {
	id, title string
	done      bool
}

type app struct {
	ctx      *sdk.Context
	once     sync.Once
	loaded   bool
	count    int
	seconds  atomic.Int64
	wifi     bool
	remember bool
	volume   float64
	scale    float64
	draft    string
	choice   int
	drop     int
	date     string
	monthY   int
	monthM   int
	expanded bool
	revealed bool
	page     int
	wide     bool
	last     string
	items    []*item
}

func (a *app) load(ctx *sdk.Context) {
	if a.loaded || ctx.StateDir == "" {
		return
	}
	a.loaded = true
	data, err := os.ReadFile(filepath.Join(ctx.StateDir, "state.json"))
	if err != nil {
		return
	}
	var saved struct {
		Count int `json:"count"`
	}
	if json.Unmarshal(data, &saved) == nil {
		a.count = saved.Count
	}
}

func (a *app) save() {
	if a.ctx == nil || a.ctx.StateDir == "" {
		return
	}
	_ = os.WriteFile(
		filepath.Join(a.ctx.StateDir, "state.json"),
		[]byte(fmt.Sprintf(`{"count":%d}`, a.count)),
		0o644,
	)
}

func text(v any) string {
	if s, ok := v.(string); ok && s != "" {
		return s
	}
	return ""
}

func (a *app) Build(ctx *sdk.Context) []sdk.Widget {
	a.once.Do(func() {
		a.ctx = ctx
		go func() {
			for range time.Tick(time.Second) {
				a.seconds.Add(1)
				ctx.Wake()
			}
		}()
	})

	a.load(ctx)

	title, _ := ctx.Options["title"].(string)
	if strings.TrimSpace(title) == "" {
		title = "Kitchen sink"
	}

	open := 0
	for _, it := range a.items {
		if !it.done {
			open++
		}
	}

	history := make([]*float64, 24)
	tooltips := make([]*string, 24)
	for i := range history {
		v := float64((int(a.seconds.Load())+i*7)%10) / 10
		history[i] = sdk.Ptr(v)
		tooltips[i] = sdk.Ptr(fmt.Sprintf("%.0f%%", v*100))
	}

	rows := make([]any, 0, len(a.items))
	for _, it := range a.items {
		rows = append(rows, sdk.ToggleRow(sdk.ToggleRowProps{
			Key: it.id, Title: it.title, Control: "check", Active: it.done,
			OnToggle: func(v bool) { it.done = v },
		}))
	}

	players := []any{}
	for _, p := range ctx.Mpris() {
		id, _ := p["id"].(string)
		players = append(players, sdk.ActionRow(sdk.ActionRowProps{
			Key: id, Icon: "media-playback-start-symbolic",
			Title:      firstText(p["title"], p["identity"], "Player"),
			Subtitle:   firstText(p["artist"], p["playback"]),
			OnActivate: func() { ctx.MprisControl(id, "play-pause") },
		}))
	}

	if len(players) == 0 {
		players = append(players, sdk.Placeholder(sdk.PlaceholderProps{
			Icon: "media-playback-stop-symbolic", Title: "No players",
			Description: "MPRIS service is empty",
		}))
	}

	workspaces := []any{}
	if compositor := ctx.Compositor(); compositor != nil {
		if list, ok := compositor["workspaces"].([]any); ok {
			for _, raw := range list {
				ws, _ := raw.(map[string]any)
				id, _ := ws["id"].(float64)
				name := text(ws["name"])
				if name == "" {
					name = fmt.Sprintf("Workspace %v", ws["id"])
				}
				workspaces = append(workspaces, sdk.ActionRow(sdk.ActionRowProps{
					Key: id, Title: name,
					Subtitle:   fmt.Sprintf("%v windows", ws["windows"]),
					OnActivate: func() { ctx.FocusWorkspace(map[string]any{"id": id}) },
				}))
			}
		}
	}

	if len(workspaces) == 0 {
		workspaces = append(workspaces, sdk.Placeholder(sdk.PlaceholderProps{
			Icon: "window-symbolic", Title: "No workspaces",
			Description: "Compositor service is empty",
		}))
	}

	vertical := ctx.Placement != nil && ctx.Placement["orientation"] == "vertical"
	indicatorText := "KS"
	if vertical {
		indicatorText = ""
	}

	dot := ""
	if a.count%2 == 1 {
		dot = "#e01b24"
	}

	return []sdk.Widget{
		sdk.Indicator(sdk.IndicatorProps{
			Icon: "applications-graphics-symbolic", Tooltip: title,
			Badge: strconv.Itoa(open), Dot: dot,
			Attention: a.count > 10,
			OnPress:   func(button int) { a.last = fmt.Sprintf("press %d", button) },
			OnScroll:  func(dx, dy float64) { a.last = fmt.Sprintf("scroll %.1f %.1f", dx, dy) },
		}, indicatorText),
		sdk.Popover(sdk.PopoverProps{
			Icon: "applications-graphics-symbolic", Title: title,
			Subtitle: fmt.Sprintf("%ds uptime", a.seconds.Load()),
			Size:     map[bool]sdk.PopoverSize{true: "wide", false: "regular"}[a.wide],
		},
			sdk.Addon(sdk.AddonProps{}, sdk.Switch(sdk.SwitchProps{
				Active: a.wide, OnToggle: func(v bool) { a.wide = v },
			})),
			sdk.Notice(sdk.NoticeProps{
				Icon: "dialog-information-symbolic", Title: "Last event",
				Subtitle: a.last, Severity: "info",
				OnActivate: func() { a.last = "notice" },
			}),
			sdk.Section(sdk.SectionProps{Title: "State and timers", Count: strconv.Itoa(a.count)},
				sdk.Row(sdk.RowProps{Icon: "timer-symbolic", Title: "Seconds", Value: strconv.FormatInt(a.seconds.Load(), 10)},
					sdk.Readout(sdk.ReadoutProps{Value: strconv.FormatInt(a.seconds.Load(), 10), Unit: "s"})),
				sdk.ActionRow(sdk.ActionRowProps{
					Icon: "list-add-symbolic", Title: "Increment persisted counter",
					Subtitle: "Saved to the state dir", Value: strconv.Itoa(a.count),
					OnActivate: func() { a.count++; a.save(); a.last = "increment" },
				}),
				sdk.Row(sdk.RowProps{Title: "Applet", Value: ctx.Name}),
				sdk.Row(sdk.RowProps{Title: "State dir", Value: map[bool]string{true: "set", false: "unset"}[ctx.StateDir != ""]},
					sdk.Spinner(sdk.SpinnerProps{})),
			),
			sdk.Section(sdk.SectionProps{Title: "Controls"},
				sdk.ToggleRow(sdk.ToggleRowProps{
					Icon: "network-wireless-symbolic", Title: "Wi-Fi", Active: a.wifi,
					OnToggle: func(v bool) { a.wifi = v },
				}),
				sdk.ToggleRow(sdk.ToggleRowProps{
					Title: "Remember", Control: "check", Active: a.remember,
					OnToggle: func(v bool) { a.remember = v },
				}),
				sdk.SliderRow(sdk.SliderRowProps{
					Title: "Volume", Value: a.volume, Min: 0.0, Max: 1.0, Step: 0.05,
					OnMove:   func(v float64) { a.last = fmt.Sprintf("move %.2f", v) },
					OnChange: func(v float64) { a.volume = v },
				}, sdk.Start(sdk.StartProps{}, sdk.Button(sdk.ButtonProps{
					Icon: "audio-volume-medium-symbolic", ClassName: "flat",
					OnClick: func() { a.last = "mute" },
				}))),
				sdk.Scale(sdk.ScaleProps{
					Value: a.scale, Min: 0.0, Max: 100.0, Step: 1.0,
					OnChange: func(v float64) { a.scale = v },
				}),
				sdk.Entry(sdk.EntryProps{
					Placeholder: "Type, then Enter", Value: a.draft,
					OnChange: func(v string) { a.draft = v },
					OnSubmit: func(v string) { a.last = "submit " + v },
				}),
				sdk.DropDown(sdk.DropDownProps{
					Options: []string{"Small", "Medium", "Large"}, Selected: sdk.Ptr(a.drop),
					OnSelect: func(i int) { a.drop = i },
				}),
				sdk.ChoiceList(sdk.ChoiceListProps{
					Choices: []sdk.Choice{
						{Label: "First", Detail: "detail one"},
						{Label: "Second", Icon: "emblem-ok-symbolic"},
						{Label: "Third", Warning: true},
					},
					Selected: sdk.Ptr(a.choice),
					OnSelect: func(i int) { a.choice = i },
				}),
				sdk.Box(sdk.BoxProps{Orientation: "horizontal", Spacing: 6},
					sdk.ToggleButton(sdk.ToggleButtonProps{
						Label: "Toggle", Active: a.wifi,
						OnToggle: func(v bool) { a.wifi = v },
					}),
					sdk.CheckButton(sdk.CheckButtonProps{
						Label: "Check", Active: a.remember,
						OnToggle: func(v bool) { a.remember = v },
					}),
					sdk.Button(sdk.ButtonProps{Text: "Plain", OnClick: func() { a.last = "button" }}),
				),
				sdk.Calendar(sdk.CalendarProps{
					Year: a.monthY, Month: a.monthM, Selected: a.date, Today: "2026-10-15",
					Events:   []sdk.CalendarEvent{{Date: "2026-10-15", Colors: []sdk.Tone{"accent"}}},
					OnMonth:  func(y, m int) { a.monthY, a.monthM = y, m },
					OnSelect: func(d string) { a.date = d },
				}),
			),
			sdk.Section(sdk.SectionProps{Title: "Keyed list", Count: strconv.Itoa(open)},
				append(rows, sdk.ActionRow(sdk.ActionRowProps{
					Title: "Rotate list", Subtitle: "Keyed rows move, state follows",
					OnActivate: func() {
						a.items = append(a.items[1:], a.items[0])
						a.last = "rotate"
					},
				}))...,
			),
			sdk.Section(sdk.SectionProps{Title: "Data"},
				sdk.MeterRow(sdk.MeterRowProps{Title: "CPU", Level: sdk.Ptr(0.3), Value: "30%"},
					sdk.Image(sdk.ImageProps{Icon: "computer-symbolic"})),
				sdk.MeterRow(sdk.MeterRowProps{Title: "Memory", Level: sdk.Ptr(0.8), Tone: "warning", Value: "80%"}),
				sdk.RangeBar(sdk.RangeBarProps{Min: 0.0, Max: 100.0, Low: 20.0, High: 80.0, Now: sdk.Ptr(float64(a.seconds.Load() % 100))}),
				sdk.Bars(sdk.BarsProps{Values: history, Maximum: sdk.Ptr(1.0), HighlightLast: sdk.Ptr(true), Label: "History", Tooltips: tooltips}),
				sdk.Sparkline(sdk.SparklineProps{Values: history, Fill: true, ShowLast: true, Label: "Trend"}),
				sdk.FactList(sdk.FactListProps{Facts: []sdk.Fact{
					{Label: "Counter", Value: strconv.Itoa(a.count)},
					{Label: "Date", Value: a.date},
				}}),
				sdk.ActionRow(sdk.ActionRowProps{Title: "Event dots", OnActivate: func() { a.last = "dots" }},
					sdk.Start(sdk.StartProps{}, sdk.Dots(sdk.DotsProps{Colors: []sdk.Tone{"accent", "success", "warning"}, Size: 10, Max: 3}))),
				sdk.ActionRow(sdk.ActionRowProps{Title: "Color swatch", OnActivate: func() { a.last = "swatch" }},
					sdk.Start(sdk.StartProps{}, sdk.Swatch(sdk.SwatchProps{Color: "#3584e4", Tooltip: "Blue"}))),
				sdk.Container(sdk.ContainerProps{Height: sdk.Ptr(44), Background: "blue", Color: "light", Align: "center"},
					sdk.Label(sdk.LabelProps{Text: "Container with a label"})),
				sdk.Separator(sdk.SeparatorProps{}),
				sdk.Label(sdk.LabelProps{Text: "A plain label", ClassName: "dim-label"}),
			),
			sdk.Section(sdk.SectionProps{Title: "Layout"},
				sdk.ToggleRow(sdk.ToggleRowProps{
					Title: "Reveal more", Active: a.revealed,
					OnToggle: func(v bool) { a.revealed = v },
				}),
				sdk.Revealer(sdk.RevealerProps{Revealed: a.revealed, Transition: "slide-down"},
					sdk.Row(sdk.RowProps{Title: "Revealed row", Subtitle: "Inside a revealer"})),
				sdk.ActionRow(sdk.ActionRowProps{
					Title: "Next stack page", Value: strconv.Itoa(a.page),
					OnActivate: func() { a.page = (a.page + 1) % 3 },
				}),
				sdk.Stack(sdk.StackProps{Visible: a.page},
					sdk.Label(sdk.LabelProps{Text: "Page one"}),
					sdk.Label(sdk.LabelProps{Text: "Page two"}),
					sdk.Label(sdk.LabelProps{Text: "Page three"})),
				sdk.ExpanderRow(sdk.ExpanderRowProps{
					Icon: "bluetooth-symbolic", Title: "Expander", Open: a.expanded,
					OnToggle: func(v bool) { a.expanded = v },
				},
					sdk.End(sdk.EndProps{}, sdk.Image(sdk.ImageProps{Icon: "object-select-symbolic"})),
					sdk.ActionRow(sdk.ActionRowProps{Title: "Drawer action", OnActivate: func() { a.last = "drawer" }}),
					sdk.MeterRow(sdk.MeterRowProps{Title: "Battery", Level: sdk.Ptr(0.6), Value: "60%"}),
					sdk.ToggleRow(sdk.ToggleRowProps{
						Title: "Drawer toggle", Active: a.wifi,
						OnToggle: func(v bool) { a.wifi = v },
					})),
			),
			sdk.Section(sdk.SectionProps{Title: "Host requests"},
				sdk.ActionRow(sdk.ActionRowProps{Title: "Notify", OnActivate: func() {
					ctx.Notify(sdk.Notification{Summary: "Kitchen sink", Body: "Hello from Go", Icon: "applications-graphics-symbolic"})
				}}),
				sdk.ActionRow(sdk.ActionRowProps{Title: "Copy draft", OnActivate: func() { ctx.Copy(firstText(a.draft, "kitchen sink")) }}),
				sdk.ActionRow(sdk.ActionRowProps{Title: "Open example.org", OnActivate: func() { ctx.OpenURI("https://example.org") }}),
				sdk.ActionRow(sdk.ActionRowProps{Title: "Launch files", OnActivate: func() { ctx.Launch("org.gnome.Nautilus") }}),
				sdk.ActionRow(sdk.ActionRowProps{Title: "Close popover", Tone: "destructive", OnActivate: func() { ctx.ClosePopover() }}),
			),
			sdk.Section(sdk.SectionProps{Title: "Players (MPRIS service)"}, players...),
			sdk.Section(sdk.SectionProps{Title: "Workspaces (compositor service)"}, workspaces...),
		),
	}
}

func firstText(values ...any) string {
	for _, v := range values {
		if s := text(v); s != "" {
			return s
		}
	}
	return ""
}

func main() {
	a := &app{
		wifi: true, volume: 0.5, scale: 30, drop: 1,
		date: "2026-10-15", monthY: 2026, monthM: 10, last: "none",
		items: []*item{{"a", "Alpha", false}, {"b", "Bravo", true}, {"c", "Charlie", false}},
	}
	sdk.Run(a)
}
