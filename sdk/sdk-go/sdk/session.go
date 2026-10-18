package sdk

import (
	"encoding/json"
	"fmt"
	"reflect"
	"slices"
	"sync"
)

type el struct {
	id         int
	kind       string
	key        any
	props      map[string]any
	handlers   map[string]Handler
	children   []*el
	appliedSeq *float64
	confirmSeq *float64
}

type App interface {
	Build(ctx *Context) []Widget
}

type Session struct {
	app       App
	WriteLine func(line string)
	OnWake    func()
	Ctx       *Context
	root      *el
	nodes     map[int]*el
	nextID    int
	Dirty     bool
	Mounted   bool
	dirtyMu   sync.Mutex
}

func (s *Session) markDirty() {
	s.dirtyMu.Lock()
	s.Dirty = true
	s.dirtyMu.Unlock()
}

func (s *Session) clearDirty() {
	s.dirtyMu.Lock()
	s.Dirty = false
	s.dirtyMu.Unlock()
}

func (s *Session) isDirty() bool {
	s.dirtyMu.Lock()
	defer s.dirtyMu.Unlock()
	return s.Dirty
}

func NewSession(app App, writeLine func(string)) *Session {
	s := &Session{
		app:       app,
		WriteLine: writeLine,
		root:      &el{kind: "root", props: map[string]any{}, handlers: map[string]Handler{}},
		nodes:     map[int]*el{},
		nextID:    1,
		Dirty:     true,
	}
	s.Ctx = &Context{session: s, Options: map[string]any{}}
	return s
}

func (s *Session) Send(msg map[string]any) {
	line, err := json.Marshal(msg)
	if err != nil {
		panic(err)
	}

	if len(line) > maxLine {
		panic(limitError("outgoing line exceeds 1 MiB"))
	}

	s.WriteLine(string(line))
}

func (s *Session) Wake() {
	if s.OnWake != nil {
		s.OnWake()
		return
	}

	s.markDirty()
}

func (s *Session) newEl(w Widget) *el {
	if len(w.Children) > maxChildren {
		panic(limitError("parent exceeds 200 children"))
	}

	node := &el{id: s.nextID, kind: w.Kind, key: w.Key, props: w.Props, handlers: w.Handlers}
	s.nextID++
	for _, child := range w.Children {
		node.children = append(node.children, s.newEl(child))
	}

	s.nodes[node.id] = node
	return node
}

func serialize(node *el) map[string]any {
	children := make([]any, len(node.children))
	for i, child := range node.children {
		children[i] = serialize(child)
	}
	return map[string]any{"id": node.id, "type": node.kind, "props": node.props, "children": children}
}

func (s *Session) forget(node *el) {
	delete(s.nodes, node.id)
	for _, child := range node.children {
		s.forget(child)
	}
}

func (s *Session) diff(node *el, w Widget, ops *[]any) {
	node.handlers = w.Handlers
	diff := map[string]any{}
	for k, newV := range w.Props {
		if oldV, ok := node.props[k]; !ok || !reflect.DeepEqual(oldV, newV) {
			diff[k] = newV
		}
	}

	for k := range node.props {
		if _, ok := w.Props[k]; !ok {
			diff[k] = nil
		}
	}

	if len(diff) > 0 {
		op := map[string]any{"op": "set", "id": node.id, "props": diff}
		if pair, ok := controlled[node.kind]; ok && node.confirmSeq != nil {
			if _, touched := diff[pair[1]]; touched {
				op["seq"] = *node.confirmSeq
				node.confirmSeq = nil
			}
		}
		*ops = append(*ops, op)
		node.props = w.Props
	}

	s.reconcile(node, w.Children, ops)
}

func (s *Session) reconcile(parent *el, widgets []Widget, ops *[]any) {
	if len(widgets) > maxChildren {
		panic(limitError("parent exceeds 200 children"))
	}

	old := parent.children
	keyed := map[any]*el{}

	var unkeyed []*el
	for _, node := range old {
		if node.key != nil {
			keyed[node.key] = node
		} else {
			unkeyed = append(unkeyed, node)
		}
	}

	type wantedItem struct {
		node  *el
		fresh bool
	}

	wanted := make([]wantedItem, 0, len(widgets))
	used := map[*el]bool{}
	cursor := 0
	for _, w := range widgets {
		var match *el
		if w.Key != nil {
			if cand, ok := keyed[w.Key]; ok && cand.kind == w.Kind && !used[cand] {
				match = cand
			}
		} else if cursor < len(unkeyed) {
			cand := unkeyed[cursor]
			cursor++
			if cand.kind == w.Kind {
				match = cand
			}
		}

		if match == nil {
			wanted = append(wanted, wantedItem{s.newEl(w), true})
		} else {
			used[match] = true
			s.diff(match, w, ops)
			wanted = append(wanted, wantedItem{match, false})
		}
	}

	for _, node := range old {
		if !used[node] {
			*ops = append(*ops, map[string]any{"op": "remove", "parent": parent.id, "id": node.id})
			s.forget(node)
		}
	}

	current := make([]*el, 0, len(wanted))
	for _, node := range old {
		if used[node] {
			current = append(current, node)
		}
	}

	for i, item := range wanted {
		var before any
		if i < len(current) {
			before = current[i].id
		}

		switch {
		case item.fresh:
			*ops = append(*ops, map[string]any{"op": "insert", "parent": parent.id, "before": before, "node": serialize(item.node)})
			current = slices.Insert(current, i, item.node)
		case i >= len(current) || current[i] != item.node:
			*ops = append(*ops, map[string]any{"op": "move", "parent": parent.id, "id": item.node.id, "before": before})
			current = slices.DeleteFunc(current, func(node *el) bool { return node == item.node })
			current = slices.Insert(current, i, item.node)
		}
	}

	parent.children = make([]*el, len(wanted))
	for i, item := range wanted {
		parent.children[i] = item.node
	}
}

func (s *Session) Rebuild() {
	var tree []Widget
	func() {
		defer func() {
			if r := recover(); r != nil {
				if _, ok := r.(limitError); ok {
					panic(r)
				}
				panic(renderError(fmt.Sprint(r)))
			}
		}()
		tree = s.app.Build(s.Ctx)
	}()

	var ops []any
	s.reconcile(s.root, tree, &ops)
	if len(s.nodes)+1 > maxNodes {
		panic(limitError("tree exceeds 300 nodes"))
	}

	for _, node := range s.nodes {
		for key, value := range node.props {
			limit := maxItems
			if key == "values" || key == "tooltips" {
				limit = maxSeries
			}
			if oversized(value, limit) {
				panic(limitError("array prop exceeds 64 items"))
			}
		}

		if node.confirmSeq != nil {
			if pair, ok := controlled[node.kind]; ok {
				ops = append(ops, map[string]any{
					"op": "set", "id": node.id,
					"props": map[string]any{pair[1]: node.props[pair[1]]},
					"seq":   *node.confirmSeq,
				})
				node.confirmSeq = nil
			}
		}
	}

	if len(ops) > maxOps {
		panic(limitError("commit exceeds 1200 operations"))
	}

	if len(ops) > 0 {
		s.Send(map[string]any{"t": "commit", "ops": ops})
	}

	s.clearDirty()
}

func (s *Session) Handle(msg map[string]any) {
	if msg["t"] == "hello" {
		if s.Mounted {
			return
		}

		if msg["v"] != float64(1) {
			panic(fmt.Sprintf("unsupported host protocol version: %v", msg["v"]))
		}

		s.Ctx.Name, _ = msg["name"].(string)
		s.Ctx.StateDir, _ = msg["state"].(string)
		if v, ok := msg["options"].(map[string]any); ok {
			s.Ctx.Options = v
		} else {
			s.Ctx.Options = map[string]any{}
		}

		s.Ctx.Placement, _ = msg["placement"].(map[string]any)
		s.Ctx.Entry = nil
		if entry, ok := msg["entry"].(map[string]any); ok {
			s.Ctx.Entry = &AppletEntry{}
			s.Ctx.Entry.ID, _ = entry["id"].(string)
			s.Ctx.Entry.Name, _ = entry["name"].(string)
			s.Ctx.Entry.Icon, _ = entry["icon"].(string)
		}

		s.Mounted = true
		s.markDirty()
		if mounter, ok := s.app.(Mounter); ok {
			mounter.Mount(s.Ctx)
		}

		if starter, ok := s.app.(Starter); ok {
			starter.Start(s.Ctx)
		}
		return
	}

	if !s.Mounted {
		return
	}

	switch msg["t"] {
	case "options":
		if v, ok := msg["options"].(map[string]any); ok {
			s.Ctx.Options = v
		} else {
			s.Ctx.Options = map[string]any{}
		}
		s.markDirty()
	case "placement":
		s.Ctx.Placement, _ = msg["placement"].(map[string]any)
		s.markDirty()
	case "popover":
		s.Ctx.PopoverOpen = msg["open"] == true
		s.markDirty()
	case "event":
		s.event(msg)
	}
}

func (s *Session) event(msg map[string]any) {
	idFloat, ok := msg["id"].(float64)
	if !ok {
		return
	}

	node := s.nodes[int(idFloat)]
	if node == nil {
		return
	}

	name, _ := msg["name"].(string)
	handler := node.handlers[name]
	if handler == nil {
		return
	}

	if pair, ok := controlled[node.kind]; ok && pair[0] == name {
		if seq, ok := msg["seq"].(float64); ok {
			if node.appliedSeq != nil && seq < *node.appliedSeq {
				return
			}
			seqCopy := seq
			node.appliedSeq = &seqCopy
			node.confirmSeq = &seqCopy
		}
	}

	args, _ := msg["args"].([]any)
	handler(args)
	s.markDirty()
}

func (s *Session) Flush() {
	if s.Mounted && s.isDirty() {
		s.Rebuild()
	}
}
