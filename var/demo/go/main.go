package main

import (
	"context"
	"fmt"
	"math"
	"path/filepath"
	"runtime"

	sdk "github.com/alex-oleshkevich/glimpse/sdk/sdk-go/sdk"
)

var profiles = []string{"Balanced", "Focus", "Presentation"}

type demoState struct {
	VPN         bool
	Quiet       bool
	Backup      bool
	Brightness  float64
	CPU         float64
	Profile     int
	Page        int
	Filter      string
	Syncs       int
	PopoverOpen bool
	LastEvent   string
}

type workstationApplet struct {
	sdk.BaseApplet[demoState]
}

func newWorkstationApplet() *workstationApplet {
	return &workstationApplet{
		BaseApplet: sdk.NewBaseApplet(demoState{
			VPN:        true,
			Backup:     true,
			Brightness: 0.68,
			CPU:        0.42,
			Profile:    0,
			Page:       1,
			Syncs:      3,
			LastEvent:  "ready",
		}),
	}
}

func (a *workstationApplet) OnStart(context.Context) error               { return nil }
func (a *workstationApplet) OnInit(context.Context, sdk.InitEvent) error { return nil }

func (a *workstationApplet) OnCallback(_ context.Context, event sdk.CallbackEvent) error {
	a.SetState(func(state *demoState) {
		switch evt := event.(type) {
		case sdk.ClickEvent:
			switch evt.ID {
			case "sync-now":
				state.Syncs++
				state.LastEvent = "manual sync requested"
			case "quiet":
				state.Quiet = !state.Quiet
				state.LastEvent = "quiet mode toggled"
			case "danger":
				state.LastEvent = "destructive action blocked in demo"
			case "open-terminal":
				state.LastEvent = "terminal shortcut selected"
			}
		case sdk.ToggleEvent:
			switch evt.ID {
			case "vpn-toggle":
				state.VPN = evt.Value
				state.LastEvent = fmt.Sprintf("vpn: %t", evt.Value)
			case "backup-toggle":
				state.Backup = evt.Value
				state.LastEvent = fmt.Sprintf("backup: %t", evt.Value)
			case "focus-toggle":
				state.Quiet = evt.Value
				state.LastEvent = fmt.Sprintf("focus: %t", evt.Value)
			}
		case sdk.ChangeEvent:
			switch evt.ID {
			case "brightness":
				state.Brightness = numericValue(evt.Value, state.Brightness)
				state.LastEvent = "brightness changed"
			case "cpu-meter":
				state.CPU = numericValue(evt.Value, state.CPU)
				state.LastEvent = "cpu changed"
			case "profile":
				state.Profile = min(selectedIndex(evt.Value), len(profiles)-1)
				state.LastEvent = "profile changed"
			}
		case sdk.InputEvent:
			if evt.ID == "filter" {
				state.Filter = evt.Text
				state.LastEvent = "filter: " + evt.Text
			}
		case sdk.ScrollEvent:
			if evt.ID == "workspace-strip" {
				delta := 2
				if evt.DeltaY > 0 {
					delta = 1
				}
				state.Page = ((state.Page + delta - 1) % 3) + 1
				state.LastEvent = fmt.Sprintf("workspace %d", state.Page)
			}
		case sdk.PopoverEvent:
			state.PopoverOpen = evt.Open
			if evt.Open {
				state.LastEvent = "popover open"
			} else {
				state.LastEvent = "popover close"
			}
		}
	})
	return nil
}

func (a *workstationApplet) Status(_ context.Context, state *demoState) ([]sdk.StatusItem, error) {
	icon := "network-vpn-symbolic"
	if !state.VPN {
		icon = "network-offline-symbolic"
	}
	return []sdk.StatusItem{{
		ID:      "workstation",
		Icon:    sdk.IconName(icon),
		Label:   profiles[state.Profile],
		Tooltip: state.LastEvent,
	}}, nil
}

func (a *workstationApplet) Popover(_ context.Context, state *demoState) (sdk.Widget, error) {
	heroSubtitle := "Popover is closing"
	if state.PopoverOpen {
		heroSubtitle = "Controls are live"
	}
	return sdk.Column{
		Spacing: 10,
		Children: []sdk.Widget{
			sdk.Hero{Title: "Workstation", Subtitle: heroSubtitle, Icon: sdk.IconName("computer-symbolic")},
			sdk.PagerStrip{
				CommonProps: sdk.CommonProps{ID: "workspace-strip", Tooltip: "Scroll to switch pages"},
				Items: []sdk.PagerItem{
					{Appearance: sdk.PagerAppearanceNumbers, Label: "1", Active: state.Page == 1, Occupied: true},
					{Appearance: sdk.PagerAppearanceNumbers, Label: "2", Active: state.Page == 2, Occupied: true},
					{Appearance: sdk.PagerAppearanceNumbers, Label: "3", Active: state.Page == 3, Urgent: state.CPU > 0.8},
				},
			},
			sdk.Grid{
				RowSpacing:    8,
				ColumnSpacing: 8,
				Children: []sdk.GridChild{
					{Row: 0, Column: 0, Child: metricCard("CPU", percent(state.CPU), "view-statistics-symbolic")},
					{Row: 0, Column: 1, Child: metricCard("Brightness", percent(state.Brightness), "display-brightness-symbolic")},
					{Row: 1, Column: 0, Child: metricCard("Syncs", fmt.Sprint(state.Syncs), "view-refresh-symbolic")},
					{Row: 1, Column: 1, Child: sdk.StatusDot{CommonProps: sdk.CommonProps{Variant: stateVariant(state.VPN)}}},
				},
			},
			controlsSection(state),
			queueSection(state),
			infoCard(state),
			activityArea(state),
			sdk.Separator{Orientation: sdk.OrientationHorizontal},
			sdk.Box{
				Orientation: sdk.OrientationHorizontal,
				Spacing:     6,
				Children: []sdk.Widget{
					sdk.Badge{Label: "SDK"},
					mutedLabel("All components covered"),
				},
			},
		},
	}, nil
}

func controlsSection(state *demoState) sdk.Widget {
	return sdk.Section{
		Title:    "Controls",
		Subtitle: "Daily workstation settings",
		Children: []sdk.Widget{
			sdk.Row{
				Spacing: 8,
				Children: []sdk.Widget{
					sdk.Button{CommonProps: sdk.CommonProps{ID: "sync-now"}, Label: "Sync", Icon: "view-refresh-symbolic", Variant: sdk.ButtonVariantPrimary},
					sdk.Button{CommonProps: sdk.CommonProps{ID: "quiet"}, Label: quietLabel(state), Icon: "notifications-disabled-symbolic", Variant: sdk.ButtonVariantSecondary},
					sdk.Button{CommonProps: sdk.CommonProps{ID: "danger"}, Label: "Reset", Icon: "edit-delete-symbolic", Enabled: boolPtr(false), Variant: sdk.ButtonVariantDanger},
				},
			},
			sdk.Switch{CommonProps: sdk.CommonProps{ID: "vpn-toggle"}, Label: "VPN tunnel", Active: state.VPN},
			sdk.ToggleButton{CommonProps: sdk.CommonProps{ID: "focus-toggle"}, Label: "Focus mode", Active: state.Quiet},
			sdk.Checkbox{CommonProps: sdk.CommonProps{ID: "backup-toggle"}, Label: "Nightly backups", Active: state.Backup},
			sdk.Slider{
				CommonProps: sdk.CommonProps{ID: "brightness"},
				Min:         0,
				Max:         1,
				Step:        0.05,
				Value:       state.Brightness,
				DrawValue:   true,
			},
			sdk.Meter{
				CommonProps: sdk.CommonProps{ID: "cpu-meter"},
				Icon:        sdk.IconName("utilities-system-monitor-symbolic"),
				Label:       "CPU pressure",
				Value:       state.CPU,
				Max:         1,
				Step:        0.01,
				Text:        percent(state.CPU),
				Interactive: true,
			},
			sdk.LevelBar{Value: state.CPU, Min: 0, Max: 1, Mode: sdk.LevelBarModeContinuous},
			sdk.MenuButton{
				Label: "Menu",
				Icon:  "open-menu-symbolic",
				Popover: sdk.Column{
					Spacing:  4,
					Children: []sdk.Widget{sdk.Label{Text: "Quick actions"}, sdk.Badge{Label: "rendered"}},
				},
			},
			sdk.Select{
				CommonProps: sdk.CommonProps{ID: "profile"},
				Items: []sdk.SelectOption{
					{ID: "0", Label: profiles[0]},
					{ID: "1", Label: profiles[1]},
					{ID: "2", Label: profiles[2]},
				},
				Selected: uintPtr(uint32(state.Profile)),
			},
		},
	}
}

func queueSection(state *demoState) sdk.Widget {
	return sdk.Section{
		Title: "Queue",
		Children: []sdk.Widget{
			sdk.ActionItem{
				CommonProps: sdk.CommonProps{ID: "open-terminal"},
				Icon:        "utilities-terminal-symbolic",
				Label:       "Terminal session",
				Sublabel:    terminalSubtitle(state),
				Right:       sdk.Button{CommonProps: sdk.CommonProps{ID: "open-terminal-indicator"}, Icon: "utilities-terminal-symbolic", Variant: sdk.ButtonVariantFlat},
			},
			sdk.ListBox{
				Children: []sdk.Widget{
					sdk.Row{Spacing: 8, Children: []sdk.Widget{sdk.Label{Text: "Build cache"}, sdk.Badge{Label: "running"}}},
					sdk.Row{Spacing: 8, Children: []sdk.Widget{sdk.Label{Text: "Backup job"}, sdk.Badge{Label: "scheduled"}}},
				},
			},
			sdk.TreeExpander{
				Child:          sdk.Label{Text: "Nested queue row"},
				HideExpander:   true,
				IndentForDepth: true,
				IndentForIcon:  true,
			},
			sdk.Section{
				Title:    "Background jobs",
				Subtitle: "Build, backup, and indexing",
				Children: []sdk.Widget{
					sdk.Row{
						Spacing: 8,
						Children: []sdk.Widget{
							sdk.Label{Text: "Index packages"},
							sdk.Label{Text: "Index packages", Wrap: true},
						},
					},
					sdk.Row{
						Spacing: 8,
						Children: []sdk.Widget{
							sdk.Label{Text: "Backup window"},
							mutedLabel(backupText(state)),
						},
					},
				},
			},
		},
	}
}

func infoCard(state *demoState) sdk.Widget {
	return sdk.Card{
		Children: []sdk.Widget{
			sdk.Row{
				Spacing: 8,
				Children: []sdk.Widget{
					sdk.Spinner{Spinning: state.Syncs%2 == 0},
					sdk.Image{Icon: sdk.IconName("dialog-information-symbolic"), PixelSize: intPtr(20)},
					sdk.Label{Text: "Filter input is handled through input callbacks.", Wrap: true},
				},
			},
			sdk.Copyable{Label: "Host", Value: "devbox.local"},
			sdk.LinkButton{URI: "https://example.com/docs", Label: "Docs"},
			sdk.Expander{
				Label:    "Session details",
				Expanded: state.PopoverOpen,
				Child: sdk.Column{
					Spacing: 4,
					Children: []sdk.Widget{
						sdk.Label{Text: "Profile: " + profiles[state.Profile]},
						sdk.Label{Text: "Last event: " + state.LastEvent},
					},
				},
			},
			sdk.Overlay{
				Child: sdk.Picture{Path: demoPicturePath(), ContentFit: sdk.ContentFitCover},
				Overlays: []sdk.Widget{
					sdk.Badge{CommonProps: sdk.CommonProps{Variant: sdk.VariantSuccess}, Label: "Live"},
				},
			},
			sdk.PropertyList{
				Title: "Session",
				Rows: sdk.Properties{
					"Profile":    profiles[state.Profile],
					"Last event": state.LastEvent,
					"Filter":     filterText(state),
				},
			},
		},
	}
}

func activityArea(state *demoState) sdk.Widget {
	if state.Filter == "" {
		return sdk.EmptyState{
			Title:    "No filtered activity",
			Subtitle: "Type in the shell-provided input callback to populate this area.",
		}
	}
	return sdk.Scroll{
		Child: sdk.Column{
			Spacing: 4,
			Children: []sdk.Widget{
				mutedLabel("Recent activity"),
				sdk.Label{Text: "VPN checked"},
				sdk.Label{Text: "Backups scheduled"},
			},
		},
	}
}

func metricCard(label, value, icon string) sdk.Widget {
	return sdk.Card{
		Children: []sdk.Widget{
			sdk.Row{
				Spacing: 6,
				Children: []sdk.Widget{
					sdk.Image{Icon: sdk.IconName(icon), PixelSize: intPtr(18)},
					sdk.Label{Text: label},
				},
			},
			sdk.Progress{Value: ratio(value), Max: 1, Text: value, ShowText: true},
		},
	}
}

func mutedLabel(text string) sdk.Label {
	return sdk.Label{CommonProps: sdk.CommonProps{Variant: sdk.VariantMuted}, Text: text}
}

func demoPicturePath() string {
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		return "../assets/workstation-picture.svg"
	}
	return filepath.Clean(filepath.Join(filepath.Dir(file), "../assets/workstation-picture.svg"))
}

func stateVariant(active bool) sdk.Variant {
	if active {
		return sdk.VariantSuccess
	}
	return sdk.VariantWarning
}

func quietLabel(state *demoState) string {
	if state.Quiet {
		return "Quiet"
	}
	return "Focus"
}

func backupText(state *demoState) string {
	if state.Backup {
		return "02:00"
	}
	return "Paused"
}

func terminalSubtitle(state *demoState) string {
	if state.VPN {
		return "Secure session"
	}
	return "Offline"
}

func filterText(state *demoState) string {
	if state.Filter == "" {
		return "none"
	}
	return state.Filter
}

func percent(value float64) string {
	return fmt.Sprintf("%.0f%%", math.Round(value*100))
}

func ratio(value string) float64 {
	var parsed float64
	if _, err := fmt.Sscanf(value, "%f%%", &parsed); err == nil {
		return parsed / 100
	}
	return 0.5
}

func numericValue(value any, fallback float64) float64 {
	if number, ok := value.(float64); ok {
		return number
	}
	return fallback
}

func selectedIndex(value any) int {
	switch typed := value.(type) {
	case float64:
		return int(typed)
	case map[string]any:
		if index, ok := typed["index"].(float64); ok {
			return int(index)
		}
	}
	return 0
}

func intPtr(value int) *int {
	return &value
}

func boolPtr(value bool) *bool {
	return &value
}

func uintPtr(value uint32) *uint32 {
	return &value
}

func main() {
	if err := sdk.Run(context.Background(), newWorkstationApplet()); err != nil {
		panic(err)
	}
}
