package sdk

import "testing"

func TestNoticePropsConvertToWirePropsAndHandler(t *testing.T) {
	activated := false
	w := Notice(NoticeProps{
		Key:        "notice-1",
		Icon:       "dialog-information-symbolic",
		Title:      "Heads up",
		Subtitle:   "Details",
		Severity:   "warning",
		ClassName:  "accent",
		OnActivate: func() { activated = true },
	})
	if w.Key != "notice-1" {
		t.Errorf("key = %v", w.Key)
	}
	want := map[string]any{
		"icon":       "dialog-information-symbolic",
		"title":      "Heads up",
		"subtitle":   "Details",
		"severity":   "warning",
		"className":  "accent",
		"onActivate": true,
	}
	for key, value := range want {
		if got := w.Props[key]; got != value {
			t.Errorf("prop %s = %v, want %v", key, got, value)
		}
	}
	handler := w.Handlers["onActivate"]
	if handler == nil {
		t.Fatal("onActivate handler missing")
	}
	handler(nil)
	if !activated {
		t.Error("handler was not called")
	}
}

func TestTypedPropsOmitZeroValuesAndKeepExplicitPointers(t *testing.T) {
	w := Bars(BarsProps{
		Values:        []*float64{Ptr(0.0), nil},
		Maximum:       Ptr(0.0),
		HighlightLast: Ptr(false),
	})
	values, ok := w.Props["values"].([]any)
	if !ok || len(values) != 2 || values[0] != 0.0 || values[1] != nil {
		t.Errorf("values = %v", w.Props["values"])
	}
	if got, ok := w.Props["maximum"]; !ok || got != 0.0 {
		t.Errorf("maximum = %v, present = %v", got, ok)
	}
	if got, ok := w.Props["highlightLast"]; !ok || got != false {
		t.Errorf("highlightLast = %v, present = %v", got, ok)
	}
	if _, ok := w.Props["label"]; ok {
		t.Error("empty label should be omitted")
	}
}

func TestTypedNestedPropsConvertToWireMaps(t *testing.T) {
	w := ChoiceList(ChoiceListProps{
		Choices:  []Choice{{Label: "First", Detail: "detail", Warning: true}},
		Selected: Ptr(0),
	})
	choices, ok := w.Props["choices"].([]any)
	if !ok || len(choices) != 1 {
		t.Fatalf("choices = %v", w.Props["choices"])
	}
	choice, ok := choices[0].(map[string]any)
	if !ok {
		t.Fatalf("choice type = %T", choices[0])
	}
	if choice["label"] != "First" || choice["detail"] != "detail" || choice["warning"] != true {
		t.Errorf("choice = %v", choice)
	}
	if _, ok := choice["icon"]; ok {
		t.Error("empty icon should be omitted")
	}
	if got := w.Props["selected"]; got != 0 {
		t.Errorf("selected = %v", got)
	}
}

func TestTypedPropsUseWireNames(t *testing.T) {
	box := Box(BoxProps{Orientation: "vertical", Hexpand: true, ClassName: "column"})
	if box.Props["hexpand"] != true || box.Props["className"] != "column" {
		t.Errorf("box props = %v", box.Props)
	}
	image := Image(ImageProps{Icon: "image-symbolic", PixelSize: Ptr(32)})
	if image.Props["pixelSize"] != 32 {
		t.Errorf("image props = %v", image.Props)
	}
}
