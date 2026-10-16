package sdk

import (
	"encoding/json"
	"os"
	"path/filepath"
	"sync"
)

type Mounter interface {
	Mount(ctx *Context)
}

type Starter interface {
	Start(ctx *Context)
}

type Applet[S any] struct {
	Initial  S
	mu       sync.Mutex
	state    S
	ready    bool
	wake     func()
	stateDir string
}

func (a *Applet[S]) ensure() {
	if !a.ready {
		a.state = a.Initial
		a.ready = true
	}
}

func (a *Applet[S]) State() S {
	a.mu.Lock()
	defer a.mu.Unlock()
	a.ensure()
	return a.state
}

func (a *Applet[S]) SetState(change func(*S)) {
	a.mu.Lock()
	a.ensure()
	change(&a.state)
	if a.stateDir != "" {
		if data, err := json.Marshal(a.state); err == nil {
			_ = os.WriteFile(filepath.Join(a.stateDir, "state.json"), data, 0o644)
		}
	}

	wake := a.wake
	a.mu.Unlock()
	if wake != nil {
		wake()
	}
}

func (a *Applet[S]) Mount(ctx *Context) {
	if ctx == nil {
		return
	}
	a.mu.Lock()
	defer a.mu.Unlock()

	a.ensure()
	if ctx.session != nil {
		a.wake = ctx.Wake
	}

	a.stateDir = ctx.StateDir
	if a.stateDir == "" {
		return
	}

	data, err := os.ReadFile(filepath.Join(a.stateDir, "state.json"))
	if err != nil {
		return
	}

	next := a.state
	if json.Unmarshal(data, &next) == nil {
		a.state = next
	}
}
