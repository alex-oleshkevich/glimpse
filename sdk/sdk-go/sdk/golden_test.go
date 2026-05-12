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

// serialized turns the SDK's structured TreeNode into a generic any tree,
// which is what the fixture parses to. This is the basis for deep-equal.
func serialized(t *testing.T, node TreeNode) any {
	t.Helper()
	data, err := json.Marshal(node)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var value any
	if err := json.Unmarshal(data, &value); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	return value
}

func assertWidget(t *testing.T, name string, node TreeNode) {
	t.Helper()
	expected := loadFixture(t, filepath.Join("widgets", name+".json"))
	got := serialized(t, node)
	if !reflect.DeepEqual(got, expected) {
		gotJSON, _ := json.MarshalIndent(got, "", "  ")
		expJSON, _ := json.MarshalIndent(expected, "", "  ")
		t.Errorf("fixture mismatch for widgets/%s.json:\ngot:\n%s\nexpected:\n%s",
			name, gotJSON, expJSON)
	}
}

func TestGoldenLabelBasic(t *testing.T)            { assertWidget(t, "label-basic", NewLabel("Hello")) }
func TestGoldenLabelModifiers(t *testing.T) {
	xalign := float32(0.5)
	assertWidget(t, "label-modifiers", TreeNode{
		Type: "label",
		Data: Label{Text: "Hello", Wrap: true, XAlign: &xalign, Selectable: true},
	})
}

func TestGoldenButtonBasic(t *testing.T) {
	assertWidget(t, "button-basic", NewButton("go", "Go"))
}

func TestGoldenButtonWithIcon(t *testing.T) {
	icon := IconName("go-symbolic")
	assertWidget(t, "button-with-icon", TreeNode{
		Type: "button",
		Data: Button{CommonProps: CommonProps{ID: "go"}, Label: "Go", Icon: icon},
	})
}

func TestGoldenButtonIconOnly(t *testing.T) {
	icon := IconName("go-symbolic")
	assertWidget(t, "button-icon-only", TreeNode{
		Type: "button",
		Data: Button{CommonProps: CommonProps{ID: "go"}, Icon: icon},
	})
}

func TestGoldenSwitchOn(t *testing.T) {
	assertWidget(t, "switch-on", TreeNode{
		Type: "switch",
		Data: Switch{CommonProps: CommonProps{ID: "vpn"}, Label: "VPN", Active: true},
	})
}

func TestGoldenSwitchOff(t *testing.T) {
	assertWidget(t, "switch-off", NewSwitch("vpn", false))
}

func TestGoldenCheckboxOn(t *testing.T) {
	assertWidget(t, "checkbox-on", TreeNode{
		Type: "checkbox",
		Data: Checkbox{CommonProps: CommonProps{ID: "autostart"}, Label: "Run at login", Active: true},
	})
}

func TestGoldenScale(t *testing.T) {
	assertWidget(t, "scale", TreeNode{
		Type: "scale",
		Data: Scale{
			CommonProps: CommonProps{ID: "brightness"},
			Min:         0.0,
			Max:         1.0,
			Step:        0.05,
			Value:       0.6,
		},
	})
}

func TestGoldenDropdown(t *testing.T) {
	selected := uint32(0)
	assertWidget(t, "dropdown", TreeNode{
		Type: "dropdown",
		Data: Dropdown{
			CommonProps: CommonProps{ID: "env"},
			Items: []DropdownItem{
				{ID: "prod", Label: "Production"},
				{ID: "stage", Label: "Staging"},
			},
			Selected: &selected,
		},
	})
}

func TestGoldenDropdownEmpty(t *testing.T) {
	assertWidget(t, "dropdown-empty", NewDropdown("env", nil))
}

func TestGoldenBadge(t *testing.T) {
	assertWidget(t, "badge", NewBadge("42%"))
}

func TestGoldenBadgeSuccessVariant(t *testing.T) {
	assertWidget(t, "badge-success-variant", TreeNode{
		Type: "badge",
		Data: Badge{CommonProps: CommonProps{Variant: VariantSuccess}, Label: "OK"},
	})
}

func TestGoldenHeroBasic(t *testing.T) {
	assertWidget(t, "hero-basic", NewHero("Counter", "Value: 0"))
}

func TestGoldenHeroWithIcon(t *testing.T) {
	icon := IconName("network-vpn-symbolic")
	assertWidget(t, "hero-with-icon", TreeNode{
		Type: "hero",
		Data: Hero{Title: "VPN", Subtitle: "Connected", Icon: icon},
	})
}

func TestGoldenProgress(t *testing.T) {
	assertWidget(t, "progress", NewProgress(0.7))
}

func TestGoldenProgressWithText(t *testing.T) {
	assertWidget(t, "progress-with-text", TreeNode{
		Type: "progress",
		Data: Progress{Value: 0.7, Max: 1.0, ShowText: true, Text: "70%"},
	})
}

func TestGoldenSpinnerDefault(t *testing.T) { assertWidget(t, "spinner-default", NewSpinner()) }
func TestGoldenSpinnerStopped(t *testing.T) {
	assertWidget(t, "spinner-stopped", NewSpinnerWith(false))
}

func TestGoldenStatusDot(t *testing.T) { assertWidget(t, "status-dot", NewStatusDot()) }
func TestGoldenStatusDotWarning(t *testing.T) {
	assertWidget(t, "status-dot-warning", TreeNode{
		Type: "status",
		Data: StatusDot{CommonProps: CommonProps{Variant: VariantWarning}},
	})
}

func TestGoldenIcon(t *testing.T) {
	pixel := 24
	icon := IconName("network-wireless-symbolic")
	assertWidget(t, "icon", TreeNode{
		Type: "icon",
		Data: IconWidget{Icon: icon, PixelSize: &pixel},
	})
}

func TestGoldenImageByName(t *testing.T) {
	icon := IconName("user-info-symbolic")
	assertWidget(t, "image-by-name", NewImage(icon))
}

func TestGoldenImageByPath(t *testing.T) {
	pixel := 64
	icon := IconPath("/home/me/avatar.png")
	assertWidget(t, "image-by-path", TreeNode{
		Type: "image",
		Data: Image{Icon: icon, PixelSize: &pixel},
	})
}

func TestGoldenSeparator(t *testing.T) { assertWidget(t, "separator", NewSeparator()) }

func TestGoldenBoxVertical(t *testing.T) {
	assertWidget(t, "box-vertical", NewBox(OrientationVertical, 8, []TreeNode{}))
}

func TestGoldenBoxHorizontal(t *testing.T) {
	assertWidget(t, "box-horizontal", NewBox(OrientationHorizontal, 4, []TreeNode{}))
}

func TestGoldenRow(t *testing.T) {
	assertWidget(t, "row", NewRow([]TreeNode{}, 8))
}

func TestGoldenColumn(t *testing.T) {
	assertWidget(t, "column", NewColumn([]TreeNode{}, 8))
}

func TestGoldenGrid(t *testing.T) {
	children := []GridChild{
		NewGridChild(0, 0, NewLabel("A")),
		{Row: 0, Column: 1, Width: 2, Height: 1, Child: NewLabel("B")},
	}
	assertWidget(t, "grid", TreeNode{
		Type: "grid",
		Data: Grid{Children: children, RowSpacing: 4, ColumnSpacing: 4},
	})
}

func TestGoldenScroll(t *testing.T) {
	assertWidget(t, "scroll", NewScroll(NewLabel("scrollable")))
}

func TestGoldenCard(t *testing.T) {
	assertWidget(t, "card", NewCard([]TreeNode{NewLabel("in card")}))
}

func TestGoldenCardEmpty(t *testing.T) {
	assertWidget(t, "card-empty", NewCard(nil))
}

func TestGoldenSectionBasic(t *testing.T) {
	assertWidget(t, "section-basic", NewSection("System", []TreeNode{NewLabel("uptime")}))
}

func TestGoldenSectionEmptyBody(t *testing.T) {
	assertWidget(t, "section-empty-body", NewSection("Empty", nil))
}

func TestGoldenCollapsibleClosed(t *testing.T) {
	assertWidget(t, "collapsible-closed", NewCollapsible("Advanced", false, nil))
}

func TestGoldenCollapsibleOpenWithBody(t *testing.T) {
	assertWidget(t, "collapsible-open-with-body",
		NewCollapsible("Advanced", true, []TreeNode{NewLabel("inside")}))
}

func TestGoldenItemBasic(t *testing.T) {
	assertWidget(t, "item-basic", NewItem("Plain"))
}

func TestGoldenItemClickable(t *testing.T) {
	assertWidget(t, "item-clickable", NewClickableItem("run", "Run"))
}

func TestGoldenItemWithMenu(t *testing.T) {
	enabled := false
	assertWidget(t, "item-with-menu", TreeNode{
		Type: "item",
		Data: Item{
			CommonProps: CommonProps{ID: "wifi-home"},
			Label:       "home-5G",
			Clickable:   true,
			Menu: []MenuItem{
				{ID: "forget", Label: "Forget"},
				{ID: "details", Label: "Details", Enabled: &enabled},
			},
		},
	})
}

func TestGoldenCollapsibleItem(t *testing.T) {
	assertWidget(t, "collapsible-item", NewCollapsibleItem("Devices", false, nil))
}

func TestGoldenActionRow(t *testing.T) {
	assertWidget(t, "action-row", NewActionRow("go", "Connect"))
}

func TestGoldenActionRowWithMeta(t *testing.T) {
	icon := IconName("network-vpn-symbolic")
	assertWidget(t, "action-row-with-meta", TreeNode{
		Type: "action_row",
		Data: ActionRow{
			CommonProps: CommonProps{ID: "go"},
			Title:       "Connect",
			Subtitle:    "wg0",
			Meta:        "4 routes",
			Icon:        icon,
		},
	})
}

func TestGoldenActionMenu(t *testing.T) {
	chkFalse := false
	chkTrue := true
	assertWidget(t, "action-menu", TreeNode{
		Type: "action_menu",
		Data: ActionMenu{
			Header: "Power profile",
			Items: []ActionMenuItem{
				{ID: "saver", Label: "Power Saver", Checked: &chkFalse},
				{ID: "balanced", Label: "Balanced", Checked: &chkTrue},
			},
		},
	})
}

func TestGoldenActionMenuEmpty(t *testing.T) {
	assertWidget(t, "action-menu-empty", NewActionMenu("", []ActionMenuItem{}))
}

func TestGoldenDetailGrid(t *testing.T) {
	assertWidget(t, "detail-grid", NewDetailGrid([]DetailGridItem{
		{Key: "SSID", Value: "home-5G"},
		{Key: "IPv4", Value: "10.0.0.42"},
	}))
}

func TestGoldenDetailGridEmpty(t *testing.T) {
	assertWidget(t, "detail-grid-empty", NewDetailGrid([]DetailGridItem{}))
}

func TestGoldenEmptyState(t *testing.T) {
	assertWidget(t, "empty-state", NewEmptyState("Nothing here"))
}

func TestGoldenEmptyStateWithSubtitle(t *testing.T) {
	assertWidget(t, "empty-state-with-subtitle", TreeNode{
		Type: "empty_state",
		Data: EmptyState{Title: "Nothing here", Subtitle: "Plug in a device."},
	})
}

func TestGoldenMeter(t *testing.T) {
	assertWidget(t, "meter", NewMeter("Memory", 0.51, 1.0))
}

func TestGoldenMeterInteractive(t *testing.T) {
	icon := IconName("audio-volume-medium-symbolic")
	assertWidget(t, "meter-interactive", TreeNode{
		Type: "meter",
		Data: Meter{
			Icon:        icon,
			Label:       "Volume",
			Value:       0.42,
			Min:         0.0,
			Max:         1.0,
			Step:        0.01,
			Text:        "42%",
			Interactive: true,
		},
	})
}

func TestGoldenCopyable(t *testing.T) {
	assertWidget(t, "copyable", NewCopyable("IPv4", "10.0.0.42"))
}

func TestGoldenToast(t *testing.T) {
	assertWidget(t, "toast", NewToast("Saved", ""))
}

func TestGoldenToastWithAction(t *testing.T) {
	icon := IconName("dialog-warning-symbolic")
	assertWidget(t, "toast-with-action", TreeNode{
		Type: "toast",
		Data: Toast{
			Icon:    icon,
			Title:   "Update available",
			Message: "Version 0.8 is available.",
			Action:  &ToastAction{ID: "update", Label: "Update"},
		},
	})
}

func TestGoldenCommonPropsAll(t *testing.T) {
	visible := false
	hex := true
	vex := true
	assertWidget(t, "common-props-all", TreeNode{
		Type: "label",
		Data: Label{
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
		},
	})
}

func TestGoldenTreeHeroColumnSection(t *testing.T) {
	tree := NewColumn([]TreeNode{
		NewHero("Counter", "Value: 0"),
		NewSection("Controls", []TreeNode{
			NewLabel("Current"),
			NewButton("increment", "Increment"),
		}),
	}, 8)
	assertWidget(t, "tree-hero-column-section", tree)
}

func TestGoldenTreeCardWithGrid(t *testing.T) {
	grid := TreeNode{
		Type: "grid",
		Data: Grid{
			Children: []GridChild{
				NewGridChild(0, 0, NewLabel("K")),
				NewGridChild(0, 1, NewBadge("V")),
			},
			RowSpacing:    4,
			ColumnSpacing: 8,
		},
	}
	assertWidget(t, "tree-card-with-grid", NewCard([]TreeNode{grid}))
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
