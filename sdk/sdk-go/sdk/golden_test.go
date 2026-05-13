// Golden cross-SDK fixture tests.
//
// Each case builds a widget and asserts its JSON serialization equals the
// corresponding fixture under ../../fixtures/widgets/.
// Event tests parse the canonical incoming payload and assert the documented
// typed event is returned.

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
	// here = .../sdk/sdk-go/sdk/golden_test.go
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

// serialized turns a Widget into a generic any tree (mirrors the fixture
// shape) for deep-equal comparison.
func serialized(t *testing.T, widget Widget) any {
	t.Helper()
	data, err := json.Marshal(widget)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var value any
	if err := json.Unmarshal(data, &value); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	return value
}

func assertWidget(t *testing.T, name string, widget Widget) {
	t.Helper()
	expected := loadFixture(t, filepath.Join("widgets", name+".json"))
	got := serialized(t, widget)
	if !reflect.DeepEqual(got, expected) {
		gotJSON, _ := json.MarshalIndent(got, "", "  ")
		expJSON, _ := json.MarshalIndent(expected, "", "  ")
		t.Errorf("fixture mismatch for widgets/%s.json:\ngot:\n%s\nexpected:\n%s",
			name, gotJSON, expJSON)
	}
}

func TestGoldenLabelBasic(t *testing.T) {
	assertWidget(t, "label-basic", Label{Text: "Hello"})
}

func TestGoldenLabelModifiers(t *testing.T) {
	xalign := float32(0.5)
	assertWidget(t, "label-modifiers", Label{
		Text:       "Hello",
		Wrap:       true,
		XAlign:     &xalign,
		Selectable: true,
	})
}

func TestGoldenButtonBasic(t *testing.T) {
	assertWidget(t, "button-basic", Button{
		CommonProps: CommonProps{ID: "go"},
		Label:       "Go",
	})
}

func TestGoldenButtonWithIcon(t *testing.T) {
	assertWidget(t, "button-with-icon", Button{
		CommonProps: CommonProps{ID: "go"},
		Label:       "Go",
		Icon:        IconName("go-symbolic"),
	})
}

func TestGoldenButtonIconOnly(t *testing.T) {
	assertWidget(t, "button-icon-only", Button{
		CommonProps: CommonProps{ID: "go"},
		Icon:        IconName("go-symbolic"),
	})
}

func TestGoldenSwitchOn(t *testing.T) {
	assertWidget(t, "switch-on", Switch{
		CommonProps: CommonProps{ID: "vpn"},
		Label:       "VPN",
		Active:      true,
	})
}

func TestGoldenSwitchOff(t *testing.T) {
	assertWidget(t, "switch-off", Switch{CommonProps: CommonProps{ID: "vpn"}})
}

func TestGoldenCheckboxOn(t *testing.T) {
	assertWidget(t, "checkbox-on", Checkbox{
		CommonProps: CommonProps{ID: "autostart"},
		Label:       "Run at login",
		Active:      true,
	})
}

func TestGoldenScale(t *testing.T) {
	assertWidget(t, "scale", Scale{
		CommonProps: CommonProps{ID: "brightness"},
		Min:         0.0,
		Max:         1.0,
		Step:        0.05,
		Value:       0.6,
	})
}

func TestGoldenDropdown(t *testing.T) {
	selected := uint32(0)
	assertWidget(t, "dropdown", Dropdown{
		CommonProps: CommonProps{ID: "env"},
		Items: []DropdownItem{
			{ID: "prod", Label: "Production"},
			{ID: "stage", Label: "Staging"},
		},
		Selected: &selected,
	})
}

func TestGoldenDropdownEmpty(t *testing.T) {
	assertWidget(t, "dropdown-empty", Dropdown{CommonProps: CommonProps{ID: "env"}})
}

func TestGoldenBadge(t *testing.T) {
	assertWidget(t, "badge", Badge{Label: "42%"})
}

func TestGoldenBadgeSuccessVariant(t *testing.T) {
	assertWidget(t, "badge-success-variant", Badge{
		CommonProps: CommonProps{Variant: VariantSuccess},
		Label:       "OK",
	})
}

func TestGoldenHeroBasic(t *testing.T) {
	assertWidget(t, "hero-basic", Hero{Title: "Counter", Subtitle: "Value: 0"})
}

func TestGoldenHeroWithIcon(t *testing.T) {
	assertWidget(t, "hero-with-icon", Hero{
		Title:    "VPN",
		Subtitle: "Connected",
		Icon:     IconName("network-vpn-symbolic"),
	})
}

func TestGoldenProgress(t *testing.T) {
	assertWidget(t, "progress", Progress{Value: 0.7, Max: 1.0})
}

func TestGoldenProgressWithText(t *testing.T) {
	assertWidget(t, "progress-with-text", Progress{
		Value:    0.7,
		Max:      1.0,
		ShowText: true,
		Text:     "70%",
	})
}

func TestGoldenSpinnerDefault(t *testing.T) {
	assertWidget(t, "spinner-default", Spinner{Spinning: true})
}

func TestGoldenSpinnerStopped(t *testing.T) {
	assertWidget(t, "spinner-stopped", Spinner{Spinning: false})
}

func TestGoldenStatusDot(t *testing.T) {
	assertWidget(t, "status-dot", StatusDot{})
}

func TestGoldenStatusDotWarning(t *testing.T) {
	assertWidget(t, "status-dot-warning", StatusDot{
		CommonProps: CommonProps{Variant: VariantWarning},
	})
}

func TestGoldenIcon(t *testing.T) {
	pixel := 24
	assertWidget(t, "icon", IconWidget{
		Icon:      IconName("network-wireless-symbolic"),
		PixelSize: &pixel,
	})
}

func TestGoldenImageByName(t *testing.T) {
	assertWidget(t, "image-by-name", Image{Icon: IconName("user-info-symbolic")})
}

func TestGoldenImageByPath(t *testing.T) {
	pixel := 64
	assertWidget(t, "image-by-path", Image{
		Icon:      IconPath("/home/me/avatar.png"),
		PixelSize: &pixel,
	})
}

func TestGoldenSeparator(t *testing.T) {
	assertWidget(t, "separator", Separator{})
}

func TestGoldenBoxVertical(t *testing.T) {
	assertWidget(t, "box-vertical", Box{
		Orientation: OrientationVertical,
		Spacing:     8,
		Children:    []Widget{},
	})
}

func TestGoldenBoxHorizontal(t *testing.T) {
	assertWidget(t, "box-horizontal", Box{
		Orientation: OrientationHorizontal,
		Spacing:     4,
		Children:    []Widget{},
	})
}

func TestGoldenRow(t *testing.T) {
	assertWidget(t, "row", Row{Spacing: 8})
}

func TestGoldenColumn(t *testing.T) {
	assertWidget(t, "column", Column{Spacing: 8})
}

func TestGoldenGrid(t *testing.T) {
	assertWidget(t, "grid", Grid{
		Children: []GridChild{
			{Row: 0, Column: 0, Width: 1, Height: 1, Child: Label{Text: "A"}},
			{Row: 0, Column: 1, Width: 2, Height: 1, Child: Label{Text: "B"}},
		},
		RowSpacing:    4,
		ColumnSpacing: 4,
	})
}

func TestGoldenScroll(t *testing.T) {
	assertWidget(t, "scroll", Scroll{Child: Label{Text: "scrollable"}})
}

func TestGoldenCard(t *testing.T) {
	assertWidget(t, "card", Card{Children: []Widget{Label{Text: "in card"}}})
}

func TestGoldenCardEmpty(t *testing.T) {
	assertWidget(t, "card-empty", Card{})
}

func TestGoldenSectionBasic(t *testing.T) {
	assertWidget(t, "section-basic", Section{
		Header: &Header{Title: "System"},
		Body:   []Widget{Label{Text: "uptime"}},
	})
}

func TestGoldenSectionEmptyBody(t *testing.T) {
	assertWidget(t, "section-empty-body", Section{Header: &Header{Title: "Empty"}})
}

func TestGoldenCollapsibleClosed(t *testing.T) {
	assertWidget(t, "collapsible-closed", Collapsible{
		Header:   &Header{Title: "Advanced"},
		Expanded: false,
	})
}

func TestGoldenCollapsibleOpenWithBody(t *testing.T) {
	assertWidget(t, "collapsible-open-with-body", Collapsible{
		Header:   &Header{Title: "Advanced"},
		Expanded: true,
		Body:     []Widget{Label{Text: "inside"}},
	})
}

func TestGoldenItemBasic(t *testing.T) {
	assertWidget(t, "item-basic", Item{Label: "Plain"})
}

func TestGoldenItemClickable(t *testing.T) {
	assertWidget(t, "item-clickable", Item{
		CommonProps: CommonProps{ID: "run"},
		Label:       "Run",
		Clickable:   true,
	})
}

func TestGoldenItemWithMenu(t *testing.T) {
	enabled := false
	assertWidget(t, "item-with-menu", Item{
		CommonProps: CommonProps{ID: "wifi-home"},
		Label:       "home-5G",
		Clickable:   true,
		Menu: []MenuItem{
			{ID: "forget", Label: "Forget"},
			{ID: "details", Label: "Details", Enabled: &enabled},
		},
	})
}

func TestGoldenCollapsibleItem(t *testing.T) {
	assertWidget(t, "collapsible-item", CollapsibleItem{Label: "Devices"})
}

func TestGoldenActionRow(t *testing.T) {
	assertWidget(t, "action-row", ActionRow{
		CommonProps: CommonProps{ID: "go"},
		Title:       "Connect",
	})
}

func TestGoldenActionRowWithMeta(t *testing.T) {
	assertWidget(t, "action-row-with-meta", ActionRow{
		CommonProps: CommonProps{ID: "go"},
		Title:       "Connect",
		Subtitle:    "wg0",
		Meta:        "4 routes",
		Icon:        IconName("network-vpn-symbolic"),
	})
}

func TestGoldenActionMenu(t *testing.T) {
	chkFalse := false
	chkTrue := true
	assertWidget(t, "action-menu", ActionMenu{
		Header: "Power profile",
		Items: []ActionMenuItem{
			{ID: "saver", Label: "Power Saver", Checked: &chkFalse},
			{ID: "balanced", Label: "Balanced", Checked: &chkTrue},
		},
	})
}

func TestGoldenActionMenuEmpty(t *testing.T) {
	assertWidget(t, "action-menu-empty", ActionMenu{})
}

func TestGoldenDetailGrid(t *testing.T) {
	assertWidget(t, "detail-grid", DetailGrid{Rows: []DetailGridItem{
		{Key: "SSID", Value: "home-5G"},
		{Key: "IPv4", Value: "10.0.0.42"},
	}})
}

func TestGoldenDetailGridEmpty(t *testing.T) {
	assertWidget(t, "detail-grid-empty", DetailGrid{})
}

func TestGoldenEmptyState(t *testing.T) {
	assertWidget(t, "empty-state", EmptyState{Title: "Nothing here"})
}

func TestGoldenEmptyStateWithSubtitle(t *testing.T) {
	assertWidget(t, "empty-state-with-subtitle", EmptyState{
		Title:    "Nothing here",
		Subtitle: "Plug in a device.",
	})
}

func TestGoldenMeter(t *testing.T) {
	assertWidget(t, "meter", Meter{
		Label: "Memory",
		Value: 0.51,
		Min:   0.0,
		Max:   1.0,
		Step:  0.01,
	})
}

func TestGoldenMeterInteractive(t *testing.T) {
	assertWidget(t, "meter-interactive", Meter{
		Icon:        IconName("audio-volume-medium-symbolic"),
		Label:       "Volume",
		Value:       0.42,
		Min:         0.0,
		Max:         1.0,
		Step:        0.01,
		Text:        "42%",
		Interactive: true,
	})
}

func TestGoldenCopyable(t *testing.T) {
	assertWidget(t, "copyable", Copyable{Label: "IPv4", Value: "10.0.0.42"})
}

func TestGoldenToast(t *testing.T) {
	assertWidget(t, "toast", Toast{Title: "Saved"})
}

func TestGoldenToastWithAction(t *testing.T) {
	assertWidget(t, "toast-with-action", Toast{
		Icon:    IconName("dialog-warning-symbolic"),
		Title:   "Update available",
		Message: "Version 0.8 is available.",
		Action:  &ToastAction{ID: "update", Label: "Update"},
	})
}

func TestGoldenCommonPropsAll(t *testing.T) {
	visible := false
	hex := true
	vex := true
	assertWidget(t, "common-props-all", Label{
		CommonProps: CommonProps{
			ID:      "marked",
			Visible: &visible,
			HExpand: &hex,
			VExpand: &vex,
			HAlign:  AlignCenter,
			VAlign:  AlignEnd,
			Tooltip: "details",
			Variant: VariantWarning,
		},
		Text: "marked",
	})
}

func TestGoldenTreeHeroColumnSection(t *testing.T) {
	assertWidget(t, "tree-hero-column-section", Column{
		Spacing: 8,
		Children: []Widget{
			Hero{Title: "Counter", Subtitle: "Value: 0"},
			Section{
				Header: &Header{Title: "Controls"},
				Body: []Widget{
					Label{Text: "Current"},
					Button{CommonProps: CommonProps{ID: "increment"}, Label: "Increment"},
				},
			},
		},
	})
}

func TestGoldenTreeCardWithGrid(t *testing.T) {
	assertWidget(t, "tree-card-with-grid", Card{
		Children: []Widget{
			Grid{
				Children: []GridChild{
					{Row: 0, Column: 0, Width: 1, Height: 1, Child: Label{Text: "K"}},
					{Row: 0, Column: 1, Width: 1, Height: 1, Child: Badge{Label: "V"}},
				},
				RowSpacing:    4,
				ColumnSpacing: 8,
			},
		},
	})
}

// ---------- events ----------

type eventFixture struct {
	Incoming map[string]any `json:"incoming"`
	Parsed   map[string]any `json:"parsed"`
}

func parseCallbackEventFromMap(t *testing.T, data map[string]any) (CallbackEvent, error) {
	t.Helper()
	encoded, err := json.Marshal(data)
	if err != nil {
		t.Fatalf("marshal incoming: %v", err)
	}
	return parseCallbackEvent(encoded)
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

func TestGoldenEventClickLeft(t *testing.T) {
	f := loadEvent(t, "click-left")
	event, err := parseCallbackEventFromMap(t, f.Incoming)
	if err != nil {
		t.Fatal(err)
	}
	c, ok := event.(ClickEvent)
	if !ok {
		t.Fatalf("expected ClickEvent, got %T", event)
	}
	if c.ID != f.Parsed["id"] {
		t.Errorf("id mismatch: %q vs %v", c.ID, f.Parsed["id"])
	}
	if c.Button != f.Parsed["button"] {
		t.Errorf("button mismatch: %q vs %v", c.Button, f.Parsed["button"])
	}
}

func TestGoldenEventClickNoButton(t *testing.T) {
	f := loadEvent(t, "click-no-button")
	event, _ := parseCallbackEventFromMap(t, f.Incoming)
	c, ok := event.(ClickEvent)
	if !ok {
		t.Fatalf("expected ClickEvent")
	}
	if c.Button != "" {
		t.Errorf("button should be empty, got %q", c.Button)
	}
}

func TestGoldenEventScrollDown(t *testing.T) {
	f := loadEvent(t, "scroll-down")
	event, _ := parseCallbackEventFromMap(t, f.Incoming)
	s, ok := event.(ScrollEvent)
	if !ok {
		t.Fatalf("expected ScrollEvent")
	}
	if s.DeltaY != f.Parsed["delta_y"].(float64) {
		t.Errorf("delta_y mismatch: %v vs %v", s.DeltaY, f.Parsed["delta_y"])
	}
}

func TestGoldenEventInput(t *testing.T) {
	f := loadEvent(t, "input")
	event, _ := parseCallbackEventFromMap(t, f.Incoming)
	i, ok := event.(InputEvent)
	if !ok {
		t.Fatalf("expected InputEvent")
	}
	if i.Text != f.Parsed["text"] {
		t.Errorf("text mismatch")
	}
}

func TestGoldenEventToggleActiveTrue(t *testing.T) {
	f := loadEvent(t, "toggle-active-true")
	event, _ := parseCallbackEventFromMap(t, f.Incoming)
	tog, ok := event.(ToggleEvent)
	if !ok || tog.Value != true {
		t.Errorf("expected toggle true, got %v", event)
	}
}

func TestGoldenEventToggleActiveFalse(t *testing.T) {
	f := loadEvent(t, "toggle-active-false")
	event, _ := parseCallbackEventFromMap(t, f.Incoming)
	tog, ok := event.(ToggleEvent)
	if !ok || tog.Value != false {
		t.Errorf("expected toggle false, got %v", event)
	}
}

func TestGoldenEventToggleViaValueTrue(t *testing.T) {
	f := loadEvent(t, "toggle-via-value-true")
	event, _ := parseCallbackEventFromMap(t, f.Incoming)
	tog, ok := event.(ToggleEvent)
	if !ok || tog.Value != true {
		t.Errorf("expected toggle true, got %v", event)
	}
}

func TestGoldenEventToggleNumericValueIsFalse(t *testing.T) {
	f := loadEvent(t, "toggle-numeric-value-is-false")
	event, _ := parseCallbackEventFromMap(t, f.Incoming)
	tog, ok := event.(ToggleEvent)
	if !ok || tog.Value != false {
		t.Errorf("expected toggle false, got %v", event)
	}
}

func TestGoldenEventChangeScale(t *testing.T) {
	f := loadEvent(t, "change-scale")
	event, _ := parseCallbackEventFromMap(t, f.Incoming)
	c, ok := event.(ChangeEvent)
	if !ok {
		t.Fatalf("expected ChangeEvent")
	}
	if !reflect.DeepEqual(c.Value, f.Parsed["value"]) {
		t.Errorf("value mismatch: got %v expected %v", c.Value, f.Parsed["value"])
	}
}

func TestGoldenEventChangeDropdown(t *testing.T) {
	f := loadEvent(t, "change-dropdown")
	event, _ := parseCallbackEventFromMap(t, f.Incoming)
	c, ok := event.(ChangeEvent)
	if !ok {
		t.Fatalf("expected ChangeEvent")
	}
	if !reflect.DeepEqual(c.Value, f.Parsed["value"]) {
		t.Errorf("value mismatch: got %v expected %v", c.Value, f.Parsed["value"])
	}
}

func TestGoldenEventPopoverOpen(t *testing.T) {
	f := loadEvent(t, "popover-open")
	event, _ := parseCallbackEventFromMap(t, f.Incoming)
	p, ok := event.(PopoverEvent)
	if !ok || !p.Open {
		t.Errorf("expected popover open")
	}
}

func TestGoldenEventPopoverClose(t *testing.T) {
	f := loadEvent(t, "popover-close")
	event, _ := parseCallbackEventFromMap(t, f.Incoming)
	p, ok := event.(PopoverEvent)
	if !ok || p.Open {
		t.Errorf("expected popover close")
	}
}
