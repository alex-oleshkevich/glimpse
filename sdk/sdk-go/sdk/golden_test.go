package sdk

import (
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"runtime"
	"testing"
)

func fixturesRoot(t *testing.T) string {
	t.Helper()
	_, here, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("runtime.Caller failed")
	}
	return filepath.Join(filepath.Dir(here), "..", "..", "fixtures")
}

func loadFixture(t *testing.T, rel string) any {
	t.Helper()
	data, err := os.ReadFile(filepath.Join(fixturesRoot(t), rel))
	if err != nil {
		t.Fatalf("read fixture %s: %v", rel, err)
	}
	var value any
	if err := json.Unmarshal(data, &value); err != nil {
		t.Fatalf("parse fixture %s: %v", rel, err)
	}
	return value
}

func serialized(t *testing.T, widget Widget) any {
	t.Helper()
	data, err := json.Marshal(widget)
	if err != nil {
		t.Fatalf("marshal widget: %v", err)
	}
	var value any
	if err := json.Unmarshal(data, &value); err != nil {
		t.Fatalf("unmarshal widget: %v", err)
	}
	return value
}

func assertWidget(t *testing.T, name string, widget Widget) {
	t.Helper()
	expected := loadFixture(t, filepath.Join("widgets", name+".json"))
	got := serialized(t, widget)
	if !reflect.DeepEqual(got, expected) {
		t.Fatalf("fixture mismatch for widgets/%s.json:\ngot %#v\nexpected %#v", name, got, expected)
	}
}

func f64ptr(v float64) *float64 { return &v }
func boolptr(v bool) *bool      { return &v }
func intptr(v int) *int         { return &v }

func sharedWidgets() map[string]Widget {
	text := Text{Text: "Ready", Size: FontSizeSm, Weight: FontWeightMedium, Color: TextColorMuted, Wrap: boolptr(true)}
	badge := Badge{Label: "OK", Kind: BadgeKindSuccess}
	status := StatusDot{Status: StatusDotWarning}
	return map[string]Widget{
		"text":   text,
		"header": Header{Label: "Network"},
		"hero": Hero{
			ID: "vpn", Icon: "network-vpn-symbolic", IconSize: intptr(32),
			Title: "VPN", Subtitle: "Disconnected", Toggle: boolptr(false),
			ToggleSensitive: boolptr(true), Separator: boolptr(true), Trailing: badge,
		},
		"badge":      badge,
		"status-dot": status,
		"panel-indicator": PanelIndicator{
			ID: "net", Icon: "network-wireless-symbolic", Label: "Wi-Fi",
			Active: true, Extra: status,
		},
		"empty-state": EmptyState{Title: "No devices", Subtitle: "Connect a device to continue"},
		"spinner":     Spinner{},
		"meter":       Meter{Label: "Memory", Value: 0.51},
		"separator":   Separator{},
		"scroll":      Scroll{Child: text},
		"row":         Row{Children: []Widget{text, badge}},
		"column":      Column{Children: []Widget{text, badge}},
		"container":  Container{Children: []Widget{text}},
		"circle-box": CircleBox{Color: "#336699"},
		"boxed-list":    BoxedList{Children: []Widget{text, badge}},
		"popover-shell": PopoverShell{Size: PopoverSizeMedium, Children: []Widget{text}, Footer: []Widget{badge}, FooterVisible: true},
		"tile": Tile{
			ID: "wifi", Primary: "Wi-Fi", Secondary: "Connected",
			LeftIcon: "network-wireless-symbolic", Right: badge, Activatable: true,
		},
		"segmented-tile": SegmentedTile{
			Tile: Tile{
				ID: "drive", Primary: "Backup", Secondary: "Mounted",
				LeftIcon: "drive-harddisk-symbolic", Right: badge, Activatable: true,
			},
			Child: KeyValueGrid{Rows: []KeyValueRow{{Key: "Size", Value: "1 TB"}}}, Expanded: true,
		},
		"button-row": ButtonRow{Children: []Widget{Tile{Primary: "Refresh", Activatable: true}}},
		"switch-tile": SwitchTile{
			ID: "bluetooth", Primary: "Bluetooth", Secondary: "On",
			LeftIcon: "bluetooth-active-symbolic", Active: true,
		},
		"expander-tile": ExpanderTile{
			ID: "details", Primary: "Details", Secondary: "2 items",
			LeftIcon: "view-list-symbolic", Child: Column{Children: []Widget{text}}, Expanded: true,
		},
		"slider-tile": SliderTile{
			ID: "brightness", Label: "Brightness", LeftIcon: "display-brightness-symbolic",
			Value: 0.6, Min: 0, Max: 1, Step: 0.05, Page: 0.1, Digits: 0, SnapStep: f64ptr(0.05),
		},
		"choice-tile": ChoiceTile{
			ID: "balanced", Primary: "Balanced", Secondary: "Recommended",
			LeftIcon: "power-profile-balanced-symbolic", Selected: true,
		},
		"choice-list": ChoiceList{
			ID: "profile", Active: "balanced",
			Choices: []Choice{
				{ID: "balanced", Primary: "Balanced", Secondary: "Recommended", Icon: "power-profile-balanced-symbolic"},
				{ID: "performance", Primary: "Performance", Secondary: "Fast", Icon: "power-profile-performance-symbolic"},
			},
		},
		"key-value-grid": KeyValueGrid{Rows: []KeyValueRow{{Key: "IPv4", Value: "10.0.0.42"}}},
		"pager-item":     PagerItem{ID: 1, Label: "1", Appearance: PagerAppearanceNumbers, Active: true, Occupied: true},
		"pager-strip": PagerStrip{
			ID: "workspaces",
			Items: []PagerItem{
				{ID: 1, Label: "1", Appearance: PagerAppearanceNumbers, Active: true, Occupied: true},
				{ID: 2, Label: "2", Appearance: PagerAppearanceNumbers, Inactive: true},
			},
		},
		"camera-indicator":      CameraIndicator{ActiveIndicator: ActiveIndicator{Active: true}},
		"mic-indicator":         MicIndicator{ActiveIndicator: ActiveIndicator{Active: true}},
		"muted-indicator":       MutedIndicator{ActiveIndicator: ActiveIndicator{Active: true}},
		"screencast-indicator":  ScreenCastIndicator{ActiveIndicator: ActiveIndicator{Active: true}, TimerText: "01:23"},
		"location-indicator":    LocationIndicator{ActiveIndicator: ActiveIndicator{Active: true}},
		"calendar":              Calendar{ID: "calendar", SelectedDate: "2026-05-22", EventDays: []string{"2026-05-22", "2026-05-24"}},
		"battery-hero":          BatteryHero{Icon: "battery-good-symbolic", Percentage: "82%", Fraction: 0.82, State: "Discharging"},
		"date-hero":             DateHero{Weekday: "Friday", Date: "May 22"},
		"events":                Events{Date: "2026-05-22", Events: []EventItem{{ID: "standup", Title: "Standup", Start: "09:30", End: "09:45"}}},
		"weather-forecast-list": WeatherForecastList{Items: []WeatherForecastItem{{DayName: "Today", Icon: "weather-clear-symbolic", Condition: "Clear", Temperatures: "12 / 20", IsToday: true}}},
		"weather-hourly-strip":  WeatherHourlyStrip{Items: []WeatherHourlyItem{{Time: "12:00", Icon: "weather-clear-symbolic", Temperature: "18"}}},
		"world-clock":           WorldClock{Rows: []WorldClockRow{{Name: "UTC", Timezone: "UTC", Time: "12:00", Offset: "+00:00", DayLabel: "Today"}}},
		"tree-shared-popover": PopoverShell{
			Size: PopoverSizeLarge,
			Children: []Widget{
				Hero{Title: "System", Subtitle: "Shared widgets"},
				BoxedList{Children: []Widget{SwitchTile{ID: "wifi", Primary: "Wi-Fi", Active: true}}},
			},
		},
	}
}

func TestGoldenWidgets(t *testing.T) {
	for name, widget := range sharedWidgets() {
		t.Run(name, func(t *testing.T) {
			assertWidget(t, name, widget)
		})
	}
}

type eventFixture struct {
	Incoming map[string]any `json:"incoming"`
	Parsed   map[string]any `json:"parsed"`
}

func loadEvent(t *testing.T, name string) eventFixture {
	t.Helper()
	data, err := os.ReadFile(filepath.Join(fixturesRoot(t), "events", name+".json"))
	if err != nil {
		t.Fatalf("read event fixture %s: %v", name, err)
	}
	var f eventFixture
	if err := json.Unmarshal(data, &f); err != nil {
		t.Fatalf("parse event fixture %s: %v", name, err)
	}
	return f
}

func eventMap(event CallbackEvent) map[string]any {
	switch e := event.(type) {
	case ClickEvent:
		var button any
		if e.Button != "" {
			button = e.Button
		}
		return map[string]any{"id": e.ID, "event": "click", "button": button}
	case ScrollEvent:
		return map[string]any{"id": e.ID, "event": "scroll", "delta_y": e.DeltaY}
	case InputEvent:
		return map[string]any{"id": e.ID, "event": "input", "text": e.Text}
	case ToggleEvent:
		return map[string]any{"id": e.ID, "event": "toggle", "value": e.Value}
	case ChangeEvent:
		return map[string]any{"id": e.ID, "event": "change", "value": e.Value}
	case PopoverEvent:
		eventName := "close"
		if e.Open {
			eventName = "open"
		}
		return map[string]any{"id": "popover", "event": eventName, "open": e.Open}
	default:
		return nil
	}
}

func TestGoldenEvents(t *testing.T) {
	files, err := os.ReadDir(filepath.Join(fixturesRoot(t), "events"))
	if err != nil {
		t.Fatal(err)
	}
	for _, file := range files {
		if filepath.Ext(file.Name()) != ".json" {
			continue
		}
		name := file.Name()[:len(file.Name())-len(".json")]
		t.Run(name, func(t *testing.T) {
			f := loadEvent(t, name)
			raw, _ := json.Marshal(f.Incoming)
			event, err := parseCallbackEvent(raw)
			if err != nil {
				t.Fatalf("parse event: %v", err)
			}
			gotMap := eventMap(event)
			if !reflect.DeepEqual(gotMap, f.Parsed) {
				t.Fatalf("event mismatch: got %#v expected %#v", gotMap, f.Parsed)
			}
		})
	}
}
