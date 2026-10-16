package sdk

import (
	"bufio"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"math"
	"net"
	"os"
	"reflect"
	"slices"
	"strings"
	"syscall"
	"time"
)

const (
	maxLine     = 1_048_576
	maxNodes    = 300
	maxChildren = 200
	maxOps      = 1200
	maxItems    = 64
	maxSeries   = 366
)

var textKinds = map[string]bool{"label": true, "indicator": true, "button": true}

var controlled = map[string][2]string{
	"entry":        {"onChange", "value"},
	"scale":        {"onChange", "value"},
	"switch":       {"onToggle", "active"},
	"togglerow":    {"onToggle", "active"},
	"togglebutton": {"onToggle", "active"},
	"checkbutton":  {"onToggle", "active"},
	"expanderrow":  {"onToggle", "open"},
	"sliderrow":    {"onChange", "value"},
	"dropdown":     {"onSelect", "selected"},
	"choicelist":   {"onSelect", "selected"},
}

type limitError string

func (e limitError) Error() string { return string(e) }

type renderError string

func (e renderError) Error() string { return string(e) }

type Props = map[string]any

type Handler func(args []any)

type Widget struct {
	Kind     string
	Key      any
	Props    map[string]any
	Handlers map[string]Handler
	Children []Widget
}

func camel(name string) string {
	parts := strings.Split(name, "_")
	for i := 1; i < len(parts); i++ {
		if parts[i] != "" {
			parts[i] = strings.ToUpper(parts[i][:1]) + parts[i][1:]
		}
	}
	return strings.Join(parts, "")
}

func clean(value any) any {
	switch v := value.(type) {
	case float64:
		if math.IsNaN(v) || math.IsInf(v, 0) {
			return nil
		}
		return v
	case float32:
		if math.IsNaN(float64(v)) || math.IsInf(float64(v), 0) {
			return nil
		}
		return float64(v)
	case []any:
		out := make([]any, len(v))
		for i, item := range v {
			out[i] = clean(item)
		}
		return out
	case map[string]any:
		out := make(map[string]any, len(v))
		for key, item := range v {
			out[key] = clean(item)
		}
		return out
	default:
		rv := reflect.ValueOf(value)
		switch rv.Kind() {
		case reflect.Slice, reflect.Array:
			out := make([]any, rv.Len())
			for i := range out {
				out[i] = clean(rv.Index(i).Interface())
			}
			return out
		case reflect.Map:
			if rv.Type().Key().Kind() == reflect.String {
				out := make(map[string]any, rv.Len())
				for _, key := range rv.MapKeys() {
					out[key.String()] = clean(rv.MapIndex(key).Interface())
				}
				return out
			}
		}
		return value
	}
}

func oversized(value any, limit int) bool {
	switch v := value.(type) {
	case []any:
		if len(v) > limit {
			return true
		}
		for _, item := range v {
			if oversized(item, maxItems) {
				return true
			}
		}
	case map[string]any:
		for _, item := range v {
			if oversized(item, maxItems) {
				return true
			}
		}
	default:
		rv := reflect.ValueOf(value)
		switch rv.Kind() {
		case reflect.Slice, reflect.Array:
			if rv.Len() > limit {
				return true
			}
			for i := 0; i < rv.Len(); i++ {
				if oversized(rv.Index(i).Interface(), maxItems) {
					return true
				}
			}
		case reflect.Map:
			for _, key := range rv.MapKeys() {
				if oversized(rv.MapIndex(key).Interface(), maxItems) {
					return true
				}
			}
		}
	}
	return false
}

func capString(s string, limit int) string {
	runes := []rune(s)
	if len(runes) > limit {
		return string(runes[:limit])
	}
	return s
}

func callFunc(fn reflect.Value, args []any) {
	t := fn.Type()
	in := make([]reflect.Value, t.NumIn())
	for i := range in {
		var arg any
		if i < len(args) {
			arg = args[i]
		}
		in[i] = convert(arg, t.In(i))
	}
	fn.Call(in)
}

func convert(arg any, target reflect.Type) reflect.Value {
	if arg == nil {
		return reflect.Zero(target)
	}
	v := reflect.ValueOf(arg)
	if v.Type().AssignableTo(target) {
		return v
	}
	if v.Type().ConvertibleTo(target) {
		return v.Convert(target)
	}
	return reflect.Zero(target)
}

func flatten(children []any, out []any) []any {
	for _, child := range children {
		switch c := child.(type) {
		case nil, bool:
			continue
		case []Widget:
			for _, w := range c {
				out = append(out, w)
			}
		case []any:
			out = flatten(c, out)
		default:
			out = append(out, child)
		}
	}
	return out
}

func W(kind string, props Props, children ...any) Widget {
	w := Widget{Kind: kind, Props: map[string]any{}, Handlers: map[string]Handler{}}
	for name, value := range props {
		if name == "key" {
			w.Key = value
			continue
		}

		wire := camel(name)
		if value != nil && reflect.ValueOf(value).Kind() == reflect.Func {
			fn := reflect.ValueOf(value)
			w.Handlers[wire] = func(args []any) { callFunc(fn, args) }
			w.Props[wire] = true
			continue
		}

		if value == nil {
			continue
		}

		capLimit := 256
		if wire == "badge" {
			capLimit = 8
		}

		if kind == "entry" && wire == "value" {
			capLimit = 4096
		}

		if s, ok := value.(string); ok {
			w.Props[wire] = capString(s, capLimit)
		} else {
			w.Props[wire] = clean(value)
		}
	}

	flat := flatten(children, nil)
	if textKinds[kind] && len(flat) > 0 {
		text := ""
		allText := true
		for _, child := range flat {
			switch c := child.(type) {
			case string:
				text += c
			case int:
				text += fmt.Sprint(c)
			case float64:
				text += fmt.Sprint(c)
			default:
				allText = false
			}
		}

		if allText {
			w.Props["text"] = capString(text, 256)
			flat = nil
		}
	}

	for _, child := range flat {
		if widget, ok := child.(Widget); ok {
			w.Children = append(w.Children, widget)
		} else {
			w.Children = append(w.Children, W("label", Props{"text": capString(fmt.Sprint(child), 256)}))
		}
	}
	return w
}

func Indicator(p Props, c ...any) Widget    { return W("indicator", p, c...) }
func Popover(p Props, c ...any) Widget      { return W("popover", p, c...) }
func Addon(p Props, c ...any) Widget        { return W("addon", p, c...) }
func Container(p Props, c ...any) Widget    { return W("container", p, c...) }
func Section(p Props, c ...any) Widget      { return W("section", p, c...) }
func Row(p Props, c ...any) Widget          { return W("row", p, c...) }
func ActionRow(p Props, c ...any) Widget    { return W("actionrow", p, c...) }
func ToggleRow(p Props, c ...any) Widget    { return W("togglerow", p, c...) }
func MeterRow(p Props, c ...any) Widget     { return W("meterrow", p, c...) }
func ExpanderRow(p Props, c ...any) Widget  { return W("expanderrow", p, c...) }
func SliderRow(p Props, c ...any) Widget    { return W("sliderrow", p, c...) }
func Start(p Props, c ...any) Widget        { return W("start", p, c...) }
func End(p Props, c ...any) Widget          { return W("end", p, c...) }
func Entry(p Props, c ...any) Widget        { return W("entry", p, c...) }
func Placeholder(p Props, c ...any) Widget  { return W("placeholder", p, c...) }
func Box(p Props, c ...any) Widget          { return W("box", p, c...) }
func Label(p Props, c ...any) Widget        { return W("label", p, c...) }
func Image(p Props, c ...any) Widget        { return W("image", p, c...) }
func Button(p Props, c ...any) Widget       { return W("button", p, c...) }
func Switch(p Props, c ...any) Widget       { return W("switch", p, c...) }
func Scale(p Props, c ...any) Widget        { return W("scale", p, c...) }
func Spinner(p Props, c ...any) Widget      { return W("spinner", p, c...) }
func Separator(p Props, c ...any) Widget    { return W("separator", p, c...) }
func FactList(p Props, c ...any) Widget     { return W("factlist", p, c...) }
func ChoiceList(p Props, c ...any) Widget   { return W("choicelist", p, c...) }
func Readout(p Props, c ...any) Widget      { return W("readout", p, c...) }
func Notice(p Props, c ...any) Widget       { return W("notice", p, c...) }
func Calendar(p Props, c ...any) Widget     { return W("calendar", p, c...) }
func Dots(p Props, c ...any) Widget         { return W("dots", p, c...) }
func DropDown(p Props, c ...any) Widget     { return W("dropdown", p, c...) }
func ToggleButton(p Props, c ...any) Widget { return W("togglebutton", p, c...) }
func CheckButton(p Props, c ...any) Widget  { return W("checkbutton", p, c...) }
func Revealer(p Props, c ...any) Widget     { return W("revealer", p, c...) }
func Stack(p Props, c ...any) Widget        { return W("stack", p, c...) }
func RangeBar(p Props, c ...any) Widget     { return W("rangebar", p, c...) }
func Bars(p Props, c ...any) Widget         { return W("bars", p, c...) }
func Sparkline(p Props, c ...any) Widget    { return W("sparkline", p, c...) }
func Swatch(p Props, c ...any) Widget       { return W("swatch", p, c...) }

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

func PositionNow(player map[string]any, nowMs float64) float64 {
	position, _ := player["positionUs"].(float64)
	if player["playback"] != "playing" {
		return position
	}

	if nowMs == 0 {
		nowMs = float64(time.Now().UnixMilli())
	}

	at, _ := player["positionAt"].(float64)
	rate, ok := player["rate"].(float64)
	if !ok {
		rate = 1
	}
	out := position + (nowMs-at)*1000*rate
	if length, ok := player["lengthUs"].(float64); ok && length > 0 && out > length {
		return length
	}
	return out
}

type Context struct {
	session     *Session
	Name        string
	StateDir    string
	Options     map[string]any
	Placement   map[string]any
	PopoverOpen bool
	Services    map[string]any
}

func (c *Context) Mpris() []map[string]any {
	value, ok := c.Services["mpris"].(map[string]any)
	if !ok {
		return nil
	}
	raw, _ := value["players"].([]any)
	players := make([]map[string]any, 0, len(raw))
	for _, item := range raw {
		if player, ok := item.(map[string]any); ok {
			players = append(players, player)
		}
	}
	return players
}

func (c *Context) Compositor() map[string]any {
	value, _ := c.Services["compositor"].(map[string]any)
	return value
}

func (c *Context) Wake() { c.session.Wake() }

func (c *Context) Notify(summary string, extra Props) {
	msg := map[string]any{"t": "notify", "summary": summary}
	for _, key := range []string{"body", "icon", "urgency"} {
		if value, ok := extra[key]; ok && value != nil {
			msg[key] = value
		}
	}
	c.session.Send(msg)
}

func (c *Context) Copy(text string) { c.session.Send(map[string]any{"t": "copy", "text": text}) }

func (c *Context) OpenURI(uri string) {
	c.session.Send(map[string]any{"t": "open-uri", "uri": uri})
}

func (c *Context) Launch(id string) { c.session.Send(map[string]any{"t": "launch", "id": id}) }

func (c *Context) ClosePopover() { c.session.Send(map[string]any{"t": "close-popover"}) }

func (c *Context) MprisControl(player, action string) {
	c.session.Send(map[string]any{"t": "mpris", "player": player, "op": "control", "action": action})
}

func (c *Context) MprisSeek(player string, offsetUs float64) {
	c.session.Send(map[string]any{"t": "mpris", "player": player, "op": "seek", "offset": math.Round(offsetUs)})
}

func (c *Context) MprisSetPosition(player string, positionUs float64) {
	c.session.Send(map[string]any{"t": "mpris", "player": player, "op": "set-position", "position": math.Round(positionUs)})
}

func (c *Context) FocusWorkspace(target any) {
	c.session.Send(map[string]any{"t": "compositor", "op": "focus-workspace", "target": target})
}

func (c *Context) FocusWindow(target any) {
	c.session.Send(map[string]any{"t": "compositor", "op": "focus-window", "target": target})
}

func (c *Context) MoveWindow(window int, target any) {
	c.session.Send(map[string]any{"t": "compositor", "op": "move-window", "window": window, "target": target})
}

func (c *Context) RenameWorkspace(id int, name *string) {
	c.session.Send(map[string]any{"t": "compositor", "op": "rename-workspace", "id": id, "name": name})
}

func (c *Context) CloseWindow(id int) {
	c.session.Send(map[string]any{"t": "compositor", "op": "close-window", "id": id})
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
	s.Ctx = &Context{session: s, Options: map[string]any{}, Services: map[string]any{}}
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
	s.Dirty = true
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
	s.Dirty = false
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
		s.Mounted = true
		s.Dirty = true
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
		s.Dirty = true
	case "placement":
		s.Ctx.Placement, _ = msg["placement"].(map[string]any)
		s.Dirty = true
	case "popover":
		s.Ctx.PopoverOpen = msg["open"] == true
		s.Dirty = true
	case "service":
		if name, ok := msg["name"].(string); ok {
			s.Ctx.Services[name] = msg["value"]
			s.Dirty = true
		}
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
	s.Dirty = true
}

func (s *Session) Flush() {
	if s.Mounted && s.Dirty {
		s.Rebuild()
	}
}

func Run(app App) {
	code := run(app)
	os.Exit(code)
}

func run(app App) (code int) {
	defer func() {
		if r := recover(); r != nil {
			fmt.Fprintln(os.Stderr, r)
			switch r.(type) {
			case limitError, renderError:
				code = 70
			default:
				code = 1
			}
		}
	}()

	path := os.Getenv("GLIMPSE_APPLET_SOCKET")
	var conn net.Conn
	var reader *bufio.Reader
	var writeLine func(string)

	if path != "" {
		dialed, err := net.Dial("unix", path)
		if err != nil {
			if errors.Is(err, fs.ErrNotExist) || errors.Is(err, syscall.ECONNREFUSED) {
				return 0
			}
			fmt.Fprintln(os.Stderr, err)
			return 1
		}
		conn = dialed
		defer conn.Close()

		reader = bufio.NewReader(conn)
		writeLine = func(line string) { _, _ = conn.Write([]byte(line + "\n")) }
	} else {
		stdout := os.Stdout
		os.Stdout = os.Stderr
		reader = bufio.NewReader(os.Stdin)
		writeLine = func(line string) { _, _ = stdout.WriteString(line + "\n") }
	}

	lines := make(chan string, 16)
	go func() {
		for {
			line, err := reader.ReadString('\n')
			if line != "" {
				lines <- line
			}
			if err != nil {
				close(lines)
				return
			}
		}
	}()

	wake := make(chan struct{}, 1)
	session := NewSession(app, writeLine)
	session.OnWake = func() {
		select {
		case wake <- struct{}{}:
		default:
		}
	}
	session.Send(map[string]any{"t": "hello", "v": 1})

	closed := false
	for {
		var batch []string
		select {
		case line, ok := <-lines:
			if !ok {
				return 0
			}
			batch = append(batch, line)
		case <-wake:
			session.Dirty = true
		}
	drain:
		for {
			select {
			case line, ok := <-lines:
				if !ok {
					closed = true
					break drain
				}
				batch = append(batch, line)
			case <-wake:
				session.Dirty = true
			default:
				break drain
			}
		}
		for _, line := range batch {
			if strings.TrimSpace(line) == "" {
				continue
			}
			var msg map[string]any
			if err := json.Unmarshal([]byte(line), &msg); err != nil {
				fmt.Fprintln(os.Stderr, err)
				continue
			}
			if _, ok := msg["t"].(string); ok {
				session.Handle(msg)
			}
		}
		session.Flush()
		if closed {
			return 0
		}
	}
}
