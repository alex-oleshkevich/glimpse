package sdk

import (
	"encoding/json"
	"strings"
	"testing"
)

type item struct {
	id, title string
	done      bool
}

type testApp struct {
	items []*item
	draft string
}

func (a *testApp) Build(ctx *Context) []Widget {
	rows := make([]any, 0, len(a.items))
	for _, it := range a.items {
		rows = append(rows, ToggleRow(ToggleRowProps{
			Key: it.id, Title: it.title, Control: "check", Active: it.done,
			OnToggle: func(v bool) { it.done = v },
		}))
	}
	return []Widget{
		Indicator(IndicatorProps{Icon: "view-list-symbolic", Badge: "2"}, "Todo"),
		Popover(PopoverProps{Title: "Todo"},
			Section(SectionProps{Title: "Todo"}, rows),
			Entry(EntryProps{Placeholder: "Add", Value: a.draft,
				OnChange: func(v string) { a.draft = v }}),
		),
	}
}

var hello = map[string]any{
	"t": "hello", "v": float64(1), "name": "test", "state": "/tmp/x",
	"options":   map[string]any{},
	"placement": map[string]any{"output": nil, "position": "bottom", "orientation": "horizontal", "zone": "left", "size": float64(40)},
}

type node struct {
	ID       int            `json:"id"`
	Type     string         `json:"type"`
	Props    map[string]any `json:"props"`
	Children []*node        `json:"children"`
}

type host struct {
	t       *testing.T
	app     *testApp
	session *Session
	lines   *[]string
	nodes   map[int]*node
	root    *node
}

func newHost(t *testing.T) *host {
	t.Helper()
	app := &testApp{items: []*item{{"a", "Alpha", false}, {"b", "Bravo", false}}}
	lines := &[]string{}
	h := &host{
		t: t, app: app, lines: lines,
		nodes: map[int]*node{},
		root:  &node{Type: "root", Props: map[string]any{}},
	}
	h.nodes[0] = h.root
	h.session = NewSession(app, func(line string) { *lines = append(*lines, line) })
	h.session.Send(map[string]any{"t": "hello", "v": 1})
	h.feed(hello)
	return h
}

func (h *host) feed(msg map[string]any) []map[string]any {
	h.t.Helper()
	h.session.Handle(msg)
	h.session.Flush()
	var commits []map[string]any
	kept := (*h.lines)[:0]
	for _, line := range *h.lines {
		if strings.Contains(line, `"commit"`) {
			var commit map[string]any
			if err := json.Unmarshal([]byte(line), &commit); err != nil {
				h.t.Fatal(err)
			}
			commits = append(commits, commit)
			h.apply(commit["ops"].([]any))
		} else {
			kept = append(kept, line)
		}
	}
	*h.lines = kept
	return commits
}

func (h *host) apply(ops []any) {
	parse := func(raw any) *node {
		data, _ := json.Marshal(raw)
		var n node
		if err := json.Unmarshal(data, &n); err != nil {
			h.t.Fatal(err)
		}
		var walk func(*node)
		walk = func(cur *node) {
			h.nodes[cur.ID] = cur
			for _, child := range cur.Children {
				walk(child)
			}
		}
		walk(&n)
		return &n
	}
	insertAt := func(parent *node, n *node, before any) {
		index := len(parent.Children)
		if before != nil {
			for i, child := range parent.Children {
				if float64(child.ID) == before.(float64) {
					index = i
				}
			}
		}
		parent.Children = append(parent.Children[:index], append([]*node{n}, parent.Children[index:]...)...)
	}
	for _, raw := range ops {
		op := raw.(map[string]any)
		switch op["op"] {
		case "insert":
			insertAt(h.nodes[int(op["parent"].(float64))], parse(op["node"]), op["before"])
		case "remove":
			parent := h.nodes[int(op["parent"].(float64))]
			id := int(op["id"].(float64))
			kept := parent.Children[:0]
			for _, child := range parent.Children {
				if child.ID != id {
					kept = append(kept, child)
				}
			}
			parent.Children = kept
		case "move":
			id := int(op["id"].(float64))
			n := h.nodes[id]
			for _, parent := range h.nodes {
				kept := parent.Children[:0]
				for _, child := range parent.Children {
					if child.ID != id {
						kept = append(kept, child)
					}
				}
				parent.Children = kept
			}
			insertAt(h.nodes[int(op["parent"].(float64))], n, op["before"])
		case "set":
			props := h.nodes[int(op["id"].(float64))].Props
			for key, value := range op["props"].(map[string]any) {
				if value == nil {
					delete(props, key)
				} else {
					props[key] = value
				}
			}
		}
	}
}

func (h *host) find(kind, title string) *node {
	h.t.Helper()
	for _, n := range h.nodes {
		if n.Type == kind && (title == "" || n.Props["title"] == title) {
			return n
		}
	}
	h.t.Fatalf("no %s %q", kind, title)
	return nil
}

func opsOf(commits []map[string]any) []map[string]any {
	var out []map[string]any
	for _, commit := range commits {
		for _, raw := range commit["ops"].([]any) {
			out = append(out, raw.(map[string]any))
		}
	}
	return out
}

func TestMountBuildsTree(t *testing.T) {
	h := newHost(t)
	if got := h.find("indicator", "").Props["text"]; got != "Todo" {
		t.Errorf("indicator text = %v", got)
	}
	if got := h.find("togglerow", "Alpha").Props["onToggle"]; got != true {
		t.Errorf("onToggle = %v", got)
	}
}

func TestToggleSetsOnlyChangedPropWithSeq(t *testing.T) {
	h := newHost(t)
	n := h.find("togglerow", "Alpha")
	ops := opsOf(h.feed(map[string]any{
		"t": "event", "id": float64(n.ID), "name": "onToggle",
		"args": []any{true}, "seq": float64(7),
	}))
	if len(ops) != 1 || ops[0]["op"] != "set" || ops[0]["seq"] != float64(7) {
		t.Fatalf("ops = %v", ops)
	}
	if !h.app.items[0].done {
		t.Error("item not toggled")
	}
}

func TestStaleSeqDropped(t *testing.T) {
	h := newHost(t)
	n := h.find("togglerow", "Alpha")
	event := func(v bool, seq float64) {
		h.feed(map[string]any{"t": "event", "id": float64(n.ID), "name": "onToggle", "args": []any{v}, "seq": seq})
	}
	event(true, 7)
	event(false, 3)
	if !h.app.items[0].done {
		t.Error("stale event applied")
	}
}

func TestKeyedReorderMoves(t *testing.T) {
	h := newHost(t)
	before := h.find("togglerow", "Alpha").ID
	h.app.items[0], h.app.items[1] = h.app.items[1], h.app.items[0]
	ops := opsOf(h.feed(map[string]any{"t": "popover", "open": true}))
	moved := false
	for _, op := range ops {
		if op["op"] == "move" {
			moved = true
		}
	}
	if !moved {
		t.Fatalf("no move op in %v", ops)
	}
	if got := h.find("togglerow", "Alpha").ID; got != before {
		t.Errorf("id changed %d -> %d", before, got)
	}
	section := h.find("section", "")
	if section.Children[0].Props["title"] != "Bravo" {
		t.Errorf("order = %v", section.Children[0].Props["title"])
	}
}

func TestAllHostRequestsReachTheWire(t *testing.T) {
	h := newHost(t)
	ctx := h.session.Ctx
	ctx.Notify(Notification{Summary: "Hi", Body: "b", Urgency: "low"})
	ctx.Copy("x")
	ctx.OpenURI("https://example.org")
	ctx.Launch("org.example.App")
	ctx.ClosePopover()
	ctx.MprisControl("p", "play-pause")
	ctx.MprisSeek("p", -5_000_000)
	ctx.MprisSetPosition("p", 12_000_000)
	ctx.FocusWorkspace(map[string]any{"index": 1})
	ctx.FocusWindow("next")
	ctx.MoveWindow(7, map[string]any{"name": "web"})
	ctx.RenameWorkspace(3, nil)
	ctx.CloseWindow(7)
	var kinds []string
	for _, line := range *h.lines {
		var msg map[string]any
		if err := json.Unmarshal([]byte(line), &msg); err != nil {
			t.Fatal(err)
		}
		if msg["t"] != "hello" {
			kinds = append(kinds, msg["t"].(string))
		}
	}
	want := "notify,copy,open-uri,launch,close-popover,mpris,mpris,mpris,compositor,compositor,compositor,compositor,compositor"
	if strings.Join(kinds, ",") != want {
		t.Errorf("kinds = %v", kinds)
	}
}

func TestCapsAndNonFinite(t *testing.T) {
	w := Indicator(IndicatorProps{Badge: "123456789", Tooltip: strings.Repeat("x", 300)})
	if len([]rune(w.Props["badge"].(string))) != 8 {
		t.Error("badge not capped at 8")
	}
	scale := Scale(ScaleProps{Value: mathNaN()})
	if scale.Props["value"] != nil {
		t.Errorf("NaN = %v", scale.Props["value"])
	}
}

func mathNaN() float64 {
	zero := 0.0
	return zero / zero
}

func TestRenderPanicIsRenderError(t *testing.T) {
	session := NewSession(boomApp{}, func(string) {})
	session.Handle(hello)
	defer func() {
		if _, ok := recover().(renderError); !ok {
			t.Error("expected renderError panic")
		}
	}()
	session.Flush()
}

type boomApp struct{}

func (boomApp) Build(ctx *Context) []Widget { panic("boom") }

func TestPositionNow(t *testing.T) {
	player := map[string]any{
		"playback": "playing", "positionUs": float64(1_000_000),
		"positionAt": float64(1000), "rate": float64(1),
	}
	if got := PositionNow(player, 3000); got != 3_000_000 {
		t.Errorf("position = %v", got)
	}
}

type wideApp struct{}

func (wideApp) Build(ctx *Context) []Widget {
	labels := make([]any, 250)
	for i := range labels {
		labels[i] = Label(LabelProps{Text: "x"})
	}
	return []Widget{Section(SectionProps{Title: "x"}, labels)}
}

func TestFreshSubtreeOverChildrenCapPanicsLimit(t *testing.T) {
	session := NewSession(wideApp{}, func(string) {})
	session.Handle(hello)
	defer func() {
		if _, ok := recover().(limitError); !ok {
			t.Error("expected limitError panic")
		}
	}()
	session.Flush()
}

func TestTypedSliceCapAndClean(t *testing.T) {
	w := DropDown(DropDownProps{Options: make([]string, 65)})
	if !oversized(w.Props["options"], maxItems) {
		t.Error("typed slice over cap not detected")
	}
	bars := Bars(BarsProps{Values: []*float64{Ptr(mathNaN())}})
	values, ok := bars.Props["values"].([]any)
	if !ok || values[0] != nil {
		t.Errorf("typed slice NaN = %v", bars.Props["values"])
	}
}
