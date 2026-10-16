package sdk

import (
	"fmt"
	"math"
	"reflect"
	"strings"
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

func capLimit(kind, wire string) int {
	if wire == "badge" {
		return 8
	}

	if kind == "entry" && wire == "value" {
		return 4096
	}

	return 256
}

func wireField(field reflect.StructField) (string, bool, bool) {
	tag := field.Tag.Get("glimpse")
	if tag == "-" {
		return "", false, true
	}

	name, options, _ := strings.Cut(tag, ",")
	if name == "" {
		name = strings.ToLower(field.Name[:1]) + field.Name[1:]
	}

	return name, options == "always", false
}

func wireValue(value any) any {
	if value == nil {
		return nil
	}

	rv := reflect.ValueOf(value)
	switch rv.Kind() {
	case reflect.Pointer:
		if rv.IsNil() {
			return nil
		}
		return wireValue(rv.Elem().Interface())
	case reflect.Struct:
		out := make(map[string]any, rv.NumField())
		appendStructValues(out, rv)
		return out
	case reflect.Slice, reflect.Array:
		out := make([]any, rv.Len())
		for i := range out {
			out[i] = wireValue(rv.Index(i).Interface())
		}
		return out
	case reflect.Map:
		if rv.Type().Key().Kind() == reflect.String {
			out := make(map[string]any, rv.Len())
			for _, key := range rv.MapKeys() {
				out[key.String()] = wireValue(rv.MapIndex(key).Interface())
			}
			return out
		}
	case reflect.String:
		return rv.String()
	}
	return clean(value)
}

func appendStructValues(out map[string]any, rv reflect.Value) {
	t := rv.Type()
	for i := 0; i < rv.NumField(); i++ {
		field := t.Field(i)
		if field.PkgPath != "" {
			continue
		}

		fv := rv.Field(i)
		if field.Anonymous {
			if fv.Kind() == reflect.Pointer && !fv.IsNil() {
				fv = fv.Elem()
			}
			if fv.Kind() == reflect.Struct {
				appendStructValues(out, fv)
				continue
			}
		}

		name, always, skip := wireField(field)
		if skip || (!always && fv.IsZero()) {
			continue
		}
		out[name] = wireValue(fv.Interface())
	}
}

func applyStructProps(w *Widget, rv reflect.Value) {
	t := rv.Type()
	for i := 0; i < rv.NumField(); i++ {
		field := t.Field(i)
		if field.PkgPath != "" {
			continue
		}

		fv := rv.Field(i)
		if field.Anonymous {
			if fv.Kind() == reflect.Pointer && !fv.IsNil() {
				fv = fv.Elem()
			}
			if fv.Kind() == reflect.Struct {
				applyStructProps(w, fv)
				continue
			}
		}

		if field.Name == "Key" {
			if !fv.IsZero() {
				w.Key = fv.Interface()
			}
			continue
		}

		name, always, skip := wireField(field)
		if skip {
			continue
		}

		if fv.Kind() == reflect.Func {
			if !fv.IsNil() {
				fn := fv
				w.Handlers[name] = func(args []any) { callFunc(fn, args) }
				w.Props[name] = true
			}
			continue
		}

		if !always && fv.IsZero() {
			continue
		}

		value := fv.Interface()
		if fv.Kind() == reflect.Pointer {
			value = fv.Elem().Interface()
		}

		rvv := reflect.ValueOf(value)
		if rvv.IsValid() && rvv.Kind() == reflect.String {
			w.Props[name] = capString(rvv.String(), capLimit(w.Kind, name))
			continue
		}

		w.Props[name] = wireValue(value)
	}
}

func applyMapProps(w *Widget, props map[string]any) {
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

		rv := reflect.ValueOf(value)
		if rv.Kind() == reflect.String {
			w.Props[wire] = capString(rv.String(), capLimit(w.Kind, wire))
		} else {
			w.Props[wire] = clean(value)
		}
	}
}

func applyProps(w *Widget, props any) {
	switch p := props.(type) {
	case nil:
	case map[string]any:
		applyMapProps(w, p)
	default:
		rv := reflect.ValueOf(props)
		if rv.Kind() == reflect.Pointer {
			if rv.IsNil() {
				return
			}
			rv = rv.Elem()
		}
		if rv.Kind() != reflect.Struct {
			panic(renderError(fmt.Sprintf("props for %s must be a struct", w.Kind)))
		}
		applyStructProps(w, rv)
	}
}

func W(kind string, props any, children ...any) Widget {
	w := Widget{Kind: kind, Props: map[string]any{}, Handlers: map[string]Handler{}}
	applyProps(&w, props)

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
			w.Children = append(w.Children, Label(LabelProps{Text: capString(fmt.Sprint(child), 256)}))
		}
	}
	return w
}
