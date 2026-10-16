package sdk

import (
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
)

type appletState struct {
	Count   int    `json:"count"`
	Seconds int    `json:"-"`
	Name    string `json:"name"`
}

type appletApp struct {
	Applet[appletState]
	starts     int
	startCount int
}

func (a *appletApp) Start(ctx *Context) {
	a.starts++
	a.startCount = a.State().Count
}

func (a *appletApp) Build(ctx *Context) []Widget {
	return []Widget{Indicator(IndicatorProps{Icon: "x"}, "hi")}
}

func appletHello(dir string) map[string]any {
	return map[string]any{
		"t": "hello", "v": float64(1), "name": "test", "state": dir,
		"options":   map[string]any{},
		"placement": map[string]any{"output": nil, "position": "bottom", "orientation": "horizontal", "zone": "left", "size": float64(40)},
	}
}

func TestSetStateWakes(t *testing.T) {
	app := &appletApp{}
	session := NewSession(app, func(string) {})
	session.Handle(appletHello(t.TempDir()))
	session.Flush()
	if session.Dirty {
		t.Fatal("session still dirty after flush")
	}
	app.SetState(func(s *appletState) { s.Count++ })
	if !session.Dirty {
		t.Error("SetState did not wake a rebuild")
	}
	if got := app.State().Count; got != 1 {
		t.Errorf("count = %d, want 1", got)
	}
}

func TestConcurrentSetState(t *testing.T) {
	app := &appletApp{}
	session := NewSession(app, func(string) {})
	session.Handle(appletHello(""))
	var wg sync.WaitGroup
	for range 8 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for range 100 {
				app.SetState(func(s *appletState) { s.Count++ })
				_ = app.State()
			}
		}()
	}
	wg.Wait()
	if got := app.State().Count; got != 800 {
		t.Errorf("count = %d, want 800", got)
	}
}

func TestAppletPersistenceRoundTrip(t *testing.T) {
	dir := t.TempDir()
	app := &appletApp{}
	app.Initial = appletState{Name: "first"}
	session := NewSession(app, func(string) {})
	session.Handle(appletHello(dir))
	app.SetState(func(s *appletState) {
		s.Count = 42
		s.Seconds = 99
	})
	data, err := os.ReadFile(filepath.Join(dir, "state.json"))
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(data), "Seconds") || strings.Contains(string(data), "99") {
		t.Errorf("skipped field persisted: %s", data)
	}
	loaded := &appletApp{}
	loaded.Initial = appletState{Name: "fallback"}
	second := NewSession(loaded, func(string) {})
	second.Handle(appletHello(dir))
	if got := loaded.State(); got.Count != 42 || got.Name != "first" || got.Seconds != 0 {
		t.Errorf("loaded = %+v, want count 42 name first seconds 0", got)
	}
	if loaded.startCount != 42 {
		t.Errorf("Start saw count %d, want loaded 42", loaded.startCount)
	}
}

func TestAppletCorruptStateIgnored(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "state.json"), []byte("{nope"), 0o644); err != nil {
		t.Fatal(err)
	}
	app := &appletApp{}
	app.Initial = appletState{Count: 7}
	session := NewSession(app, func(string) {})
	session.Handle(appletHello(dir))
	session.Flush()
	if got := app.State().Count; got != 7 {
		t.Errorf("count = %d, want initial 7", got)
	}
}

func TestAppletMissingStateIgnored(t *testing.T) {
	app := &appletApp{}
	app.Initial = appletState{Count: 3}
	session := NewSession(app, func(string) {})
	session.Handle(appletHello(t.TempDir()))
	if got := app.State().Count; got != 3 {
		t.Errorf("count = %d, want 3", got)
	}
}

func TestStartRunsOnceAfterMount(t *testing.T) {
	app := &appletApp{}
	session := NewSession(app, func(string) {})
	session.Handle(appletHello(t.TempDir()))
	session.Handle(appletHello(t.TempDir()))
	if app.starts != 1 {
		t.Errorf("starts = %d, want 1", app.starts)
	}
}

type plainApp struct{}

func (plainApp) Build(ctx *Context) []Widget {
	return []Widget{Indicator(IndicatorProps{Icon: "x"}, "hi")}
}

func TestPlainBuildOnlyAppStillWorks(t *testing.T) {
	var lines []string
	session := NewSession(plainApp{}, func(line string) { lines = append(lines, line) })
	session.Handle(appletHello(t.TempDir()))
	session.Flush()
	if len(lines) != 1 || !strings.Contains(lines[0], `"commit"`) {
		t.Fatalf("lines = %v, want one commit", lines)
	}
}
