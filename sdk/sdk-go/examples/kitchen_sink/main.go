package main

import (
	"fmt"
	"strconv"
	"strings"
	"time"

	sdk "github.com/alex-oleshkevich/glimpse/sdk/sdk-go/sdk"
)

type item struct {
	id, title string
	done      bool
}

type kitchenState struct {
	Count    int     `json:"count"`
	seconds  int64   `json:"-"`
	wifi     bool    `json:"-"`
	remember bool    `json:"-"`
	volume   float64 `json:"-"`
	scale    float64 `json:"-"`
	draft    string  `json:"-"`
	choice   int     `json:"-"`
	drop     int     `json:"-"`
	date     string  `json:"-"`
	monthY   int     `json:"-"`
	monthM   int     `json:"-"`
	expanded bool    `json:"-"`
	revealed bool    `json:"-"`
	page     int     `json:"-"`
	wide     bool    `json:"-"`
	last     string  `json:"-"`
	items    []item  `json:"-"`
}

type app struct {
	sdk.Applet[kitchenState]
}

func (a *app) Start(ctx *sdk.Context) {
	go func() {
		for range time.Tick(time.Second) {
			a.SetState(func(s *kitchenState) { s.seconds++ })
		}
	}()
}

func (a *app) setLast(value string) {
	a.SetState(func(s *kitchenState) { s.last = value })
}

func text(v any) string {
	if s, ok := v.(string); ok && s != "" {
		return s
	}
	return ""
}

func (a *app) Build(ctx *sdk.Context) []sdk.Widget {
	st := a.State()

	title, _ := ctx.Options["title"].(string)
	if strings.TrimSpace(title) == "" {
		title = "Kitchen sink"
	}

	open := 0
	for _, it := range st.items {
		if !it.done {
			open++
		}
	}

	history := make([]*float64, 24)
	tooltips := make([]*string, 24)
	for i := range history {
		v := float64((int(st.seconds)+i*7)%10) / 10
		history[i] = sdk.Ptr(v)
		tooltips[i] = sdk.Ptr(fmt.Sprintf("%.0f%%", v*100))
	}

	rows := make([]any, 0, len(st.items))
	for _, it := range st.items {
		id := it.id
		rows = append(rows, sdk.ToggleRow(sdk.ToggleRowProps{
			Key: it.id, Title: it.title, Control: "check", Active: it.done,
			OnToggle: func(v bool) {
				a.SetState(func(s *kitchenState) {
					for i := range s.items {
						if s.items[i].id == id {
							s.items[i].done = v
						}
					}
				})
			},
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
	if st.Count%2 == 1 {
		dot = "#e01b24"
	}

	return []sdk.Widget{
		sdk.Indicator(sdk.IndicatorProps{
			Icon: "applications-graphics-symbolic", Tooltip: title,
			Badge: strconv.Itoa(open), Dot: dot,
			Attention: st.Count > 10,
			OnPress:   func(button int) { a.setLast(fmt.Sprintf("press %d", button)) },
			OnScroll:  func(dx, dy float64) { a.setLast(fmt.Sprintf("scroll %.1f %.1f", dx, dy)) },
		}, indicatorText),
		sdk.Popover(sdk.PopoverProps{
			Icon: "applications-graphics-symbolic", Title: title,
			Subtitle: fmt.Sprintf("%ds uptime", st.seconds),
			Size:     map[bool]sdk.PopoverSize{true: "wide", false: "regular"}[st.wide],
		},
			sdk.Addon(sdk.AddonProps{}, sdk.Switch(sdk.SwitchProps{
				Active: st.wide, OnToggle: func(v bool) { a.SetState(func(s *kitchenState) { s.wide = v }) },
			})),
			sdk.Notice(sdk.NoticeProps{
				Icon: "dialog-information-symbolic", Title: "Last event",
				Subtitle: st.last, Severity: "info",
				OnActivate: func() { a.setLast("notice") },
			}),
			sdk.Section(sdk.SectionProps{Title: "State and timers", Count: strconv.Itoa(st.Count)},
				sdk.Row(sdk.RowProps{Icon: "timer-symbolic", Title: "Seconds", Value: strconv.FormatInt(st.seconds, 10)},
					sdk.Readout(sdk.ReadoutProps{Value: strconv.FormatInt(st.seconds, 10), Unit: "s"})),
				sdk.ActionRow(sdk.ActionRowProps{
					Icon: "list-add-symbolic", Title: "Increment persisted counter",
					Subtitle: "Saved to the state dir", Value: strconv.Itoa(st.Count),
					OnActivate: func() {
						a.SetState(func(s *kitchenState) { s.Count++; s.last = "increment" })
					},
				}),
				sdk.Row(sdk.RowProps{Title: "Applet", Value: ctx.Name}),
				sdk.Row(sdk.RowProps{Title: "State dir", Value: map[bool]string{true: "set", false: "unset"}[ctx.StateDir != ""]},
					sdk.Spinner(sdk.SpinnerProps{})),
			),
			sdk.Section(sdk.SectionProps{Title: "Controls"},
				sdk.ToggleRow(sdk.ToggleRowProps{
					Icon: "network-wireless-symbolic", Title: "Wi-Fi", Active: st.wifi,
					OnToggle: func(v bool) { a.SetState(func(s *kitchenState) { s.wifi = v }) },
				}),
				sdk.ToggleRow(sdk.ToggleRowProps{
					Title: "Remember", Control: "check", Active: st.remember,
					OnToggle: func(v bool) { a.SetState(func(s *kitchenState) { s.remember = v }) },
				}),
				sdk.SliderRow(sdk.SliderRowProps{
					Title: "Volume", Value: st.volume, Min: 0.0, Max: 1.0, Step: 0.05,
					OnMove:   func(v float64) { a.setLast(fmt.Sprintf("move %.2f", v)) },
					OnChange: func(v float64) { a.SetState(func(s *kitchenState) { s.volume = v }) },
				}, sdk.Start(sdk.StartProps{}, sdk.Button(sdk.ButtonProps{
					Icon: "audio-volume-medium-symbolic", ClassName: "flat",
					OnClick: func() { a.setLast("mute") },
				}))),
				sdk.Scale(sdk.ScaleProps{
					Value: st.scale, Min: 0.0, Max: 100.0, Step: 1.0,
					OnChange: func(v float64) { a.SetState(func(s *kitchenState) { s.scale = v }) },
				}),
				sdk.Entry(sdk.EntryProps{
					Placeholder: "Type, then Enter", Value: st.draft,
					OnChange: func(v string) { a.SetState(func(s *kitchenState) { s.draft = v }) },
					OnSubmit: func(v string) { a.setLast("submit " + v) },
				}),
				sdk.DropDown(sdk.DropDownProps{
					Options: []string{"Small", "Medium", "Large"}, Selected: sdk.Ptr(st.drop),
					OnSelect: func(i int) { a.SetState(func(s *kitchenState) { s.drop = i }) },
				}),
				sdk.ChoiceList(sdk.ChoiceListProps{
					Choices: []sdk.Choice{
						{Label: "First", Detail: "detail one"},
						{Label: "Second", Icon: "emblem-ok-symbolic"},
						{Label: "Third", Warning: true},
					},
					Selected: sdk.Ptr(st.choice),
					OnSelect: func(i int) { a.SetState(func(s *kitchenState) { s.choice = i }) },
				}),
				sdk.Box(sdk.BoxProps{Orientation: "horizontal", Spacing: 6},
					sdk.ToggleButton(sdk.ToggleButtonProps{
						Label: "Toggle", Active: st.wifi,
						OnToggle: func(v bool) { a.SetState(func(s *kitchenState) { s.wifi = v }) },
					}),
					sdk.CheckButton(sdk.CheckButtonProps{
						Label: "Check", Active: st.remember,
						OnToggle: func(v bool) { a.SetState(func(s *kitchenState) { s.remember = v }) },
					}),
					sdk.Button(sdk.ButtonProps{Text: "Plain", OnClick: func() { a.setLast("button") }}),
				),
				sdk.Calendar(sdk.CalendarProps{
					Year: st.monthY, Month: st.monthM, Selected: st.date, Today: "2026-10-15",
					Events: []sdk.CalendarEvent{{Date: "2026-10-15", Colors: []sdk.Tone{"accent"}}},
					OnMonth: func(y, m int) {
						a.SetState(func(s *kitchenState) { s.monthY, s.monthM = y, m })
					},
					OnSelect: func(d string) { a.SetState(func(s *kitchenState) { s.date = d }) },
				}),
			),
			sdk.Section(sdk.SectionProps{Title: "Keyed list", Count: strconv.Itoa(open)},
				append(rows, sdk.ActionRow(sdk.ActionRowProps{
					Title: "Rotate list", Subtitle: "Keyed rows move, state follows",
					OnActivate: func() {
						a.SetState(func(s *kitchenState) {
							if len(s.items) > 1 {
								first := s.items[0]
								copy(s.items, s.items[1:])
								s.items[len(s.items)-1] = first
							}
							s.last = "rotate"
						})
					},
				}))...,
			),
			sdk.Section(sdk.SectionProps{Title: "Data"},
				sdk.MeterRow(sdk.MeterRowProps{Title: "CPU", Level: sdk.Ptr(0.3), Value: "30%"},
					sdk.Image(sdk.ImageProps{Icon: "computer-symbolic"})),
				sdk.MeterRow(sdk.MeterRowProps{Title: "Memory", Level: sdk.Ptr(0.8), Tone: "warning", Value: "80%"}),
				sdk.RangeBar(sdk.RangeBarProps{Min: 0.0, Max: 100.0, Low: 20.0, High: 80.0, Now: sdk.Ptr(float64(st.seconds % 100))}),
				sdk.Bars(sdk.BarsProps{Values: history, Maximum: sdk.Ptr(1.0), HighlightLast: sdk.Ptr(true), Label: "History", Tooltips: tooltips}),
				sdk.Sparkline(sdk.SparklineProps{Values: history, Fill: true, ShowLast: true, Label: "Trend"}),
				sdk.FactList(sdk.FactListProps{Facts: []sdk.Fact{
					{Label: "Counter", Value: strconv.Itoa(st.Count)},
					{Label: "Date", Value: st.date},
				}}),
				sdk.ActionRow(sdk.ActionRowProps{Title: "Event dots", OnActivate: func() { a.setLast("dots") }},
					sdk.Start(sdk.StartProps{}, sdk.Dots(sdk.DotsProps{Colors: []sdk.Tone{"accent", "success", "warning"}, Size: 10, Max: 3}))),
				sdk.ActionRow(sdk.ActionRowProps{Title: "Color swatch", OnActivate: func() { a.setLast("swatch") }},
					sdk.Start(sdk.StartProps{}, sdk.Swatch(sdk.SwatchProps{Color: "#3584e4", Tooltip: "Blue"}))),
				sdk.Container(sdk.ContainerProps{Height: sdk.Ptr(44), Background: "blue", Color: "light", Align: "center"},
					sdk.Label(sdk.LabelProps{Text: "Container with a label"})),
				sdk.Separator(sdk.SeparatorProps{}),
				sdk.Label(sdk.LabelProps{Text: "A plain label", ClassName: "dim-label"}),
			),
			sdk.Section(sdk.SectionProps{Title: "Layout"},
				sdk.ToggleRow(sdk.ToggleRowProps{
					Title: "Reveal more", Active: st.revealed,
					OnToggle: func(v bool) { a.SetState(func(s *kitchenState) { s.revealed = v }) },
				}),
				sdk.Revealer(sdk.RevealerProps{Revealed: st.revealed, Transition: "slide-down"},
					sdk.Row(sdk.RowProps{Title: "Revealed row", Subtitle: "Inside a revealer"})),
				sdk.ActionRow(sdk.ActionRowProps{
					Title: "Next stack page", Value: strconv.Itoa(st.page),
					OnActivate: func() { a.SetState(func(s *kitchenState) { s.page = (s.page + 1) % 3 }) },
				}),
				sdk.Stack(sdk.StackProps{Visible: st.page},
					sdk.Label(sdk.LabelProps{Text: "Page one"}),
					sdk.Label(sdk.LabelProps{Text: "Page two"}),
					sdk.Label(sdk.LabelProps{Text: "Page three"})),
				sdk.ExpanderRow(sdk.ExpanderRowProps{
					Icon: "bluetooth-symbolic", Title: "Expander", Open: st.expanded,
					OnToggle: func(v bool) { a.SetState(func(s *kitchenState) { s.expanded = v }) },
				},
					sdk.End(sdk.EndProps{}, sdk.Image(sdk.ImageProps{Icon: "object-select-symbolic"})),
					sdk.ActionRow(sdk.ActionRowProps{Title: "Drawer action", OnActivate: func() { a.setLast("drawer") }}),
					sdk.MeterRow(sdk.MeterRowProps{Title: "Battery", Level: sdk.Ptr(0.6), Value: "60%"}),
					sdk.ToggleRow(sdk.ToggleRowProps{
						Title: "Drawer toggle", Active: st.wifi,
						OnToggle: func(v bool) { a.SetState(func(s *kitchenState) { s.wifi = v }) },
					})),
			),
			sdk.Section(sdk.SectionProps{Title: "Host requests"},
				sdk.ActionRow(sdk.ActionRowProps{Title: "Notify", OnActivate: func() {
					ctx.Notify(sdk.Notification{Summary: "Kitchen sink", Body: "Hello from Go", Icon: "applications-graphics-symbolic"})
				}}),
				sdk.ActionRow(sdk.ActionRowProps{Title: "Copy draft", OnActivate: func() { ctx.Copy(firstText(a.State().draft, "kitchen sink")) }}),
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
	a := &app{}
	a.Initial = kitchenState{
		wifi: true, volume: 0.5, scale: 30, drop: 1,
		date: "2026-10-15", monthY: 2026, monthM: 10, last: "none",
		items: []item{{"a", "Alpha", false}, {"b", "Bravo", true}, {"c", "Charlie", false}},
	}
	sdk.Main(a)
}
