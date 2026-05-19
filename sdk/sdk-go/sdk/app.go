package sdk

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strings"
	"sync"
)

type Applet[S any] interface {
	State() *S
	SetState(func(*S))
	OnStart(context.Context) error
	OnInit(context.Context, InitEvent) error
	OnCallback(context.Context, CallbackEvent) error
	Status(context.Context, *S) ([]StatusItem, error)
	Popover(context.Context, *S) (Widget, error)
	// CssClass returns the CSS class applied to the applet indicator and popover
	// (e.g. "workstation" → applet-workstation on both GTK widgets).
	// Return "" (the default from BaseApplet) for no extra class.
	CssClass() string
}

// Optional typed handler interfaces. Implement any of these on your applet
// struct instead of OnCallback and the runtime will route to them directly.
// OnCallback is still called for event types that have no matching interface.
type ClickHandler interface{ OnClick(context.Context, ClickEvent) error }
type ScrollHandler interface{ OnScroll(context.Context, ScrollEvent) error }
type InputHandler interface{ OnInput(context.Context, InputEvent) error }
type ChangeHandler interface{ OnChange(context.Context, ChangeEvent) error }
type ToggleHandler interface{ OnToggle(context.Context, ToggleEvent) error }
type PopoverHandler interface{ OnPopover(context.Context, PopoverEvent) error }

type inlineHandler func(context.Context, CallbackEvent) error

type BaseApplet[S any] struct {
	mu          sync.RWMutex
	state       S
	popoverOpen bool
	updates     chan struct{}
}

func NewBaseApplet[S any](state S) BaseApplet[S] {
	return BaseApplet[S]{
		state:   state,
		updates: make(chan struct{}, 1),
	}
}

func (a *BaseApplet[S]) State() *S {
	return &a.state
}

func (a *BaseApplet[S]) SetState(patch func(*S)) {
	a.mu.Lock()
	patch(&a.state)
	a.mu.Unlock()
	select {
	case a.updates <- struct{}{}:
	default:
	}
}

func (a *BaseApplet[S]) Snapshot() S {
	a.mu.RLock()
	defer a.mu.RUnlock()
	return a.state
}

func (a *BaseApplet[S]) IsPopoverOpen() bool {
	a.mu.RLock()
	defer a.mu.RUnlock()
	return a.popoverOpen
}

func (a *BaseApplet[S]) SetPopoverOpen(open bool) {
	a.mu.Lock()
	changed := a.popoverOpen != open
	a.popoverOpen = open
	a.mu.Unlock()
	if changed {
		select {
		case a.updates <- struct{}{}:
		default:
		}
	}
}

func (a *BaseApplet[S]) Updates() <-chan struct{} {
	return a.updates
}

func (a *BaseApplet[S]) CssClass() string { return "" }

// Log writes a debug line to stderr. In applets dev mode the line appears
// directly in the terminal; when running under the panel it is captured by
// the shell's stderr logger.
func (a *BaseApplet[S]) Log(args ...any) {
	fmt.Fprintln(os.Stderr, args...)
}

type CommandResult struct {
	Stdout string
	Stderr string
	RC     int
}

func (a *BaseApplet[S]) RunCommand(ctx context.Context, command []string) (CommandResult, error) {
	return RunCommand(ctx, command)
}

func (a *BaseApplet[S]) CopyToClipboard(ctx context.Context, text string) error {
	return CopyToClipboard(ctx, text)
}

func (a *BaseApplet[S]) OpenURI(ctx context.Context, uri string) error {
	return OpenURI(ctx, uri)
}

func (a *BaseApplet[S]) ShowNotification(ctx context.Context, summary string, body ...string) error {
	return ShowNotification(ctx, summary, body...)
}

var runDesktopCommand = func(ctx context.Context, command string, args []string, stdin string) error {
	cmd := exec.CommandContext(ctx, command, args...)
	if stdin != "" {
		cmd.Stdin = strings.NewReader(stdin)
	}
	return cmd.Run()
}

func CopyToClipboard(ctx context.Context, text string) error {
	return runDesktopCommand(ctx, "wl-copy", nil, text)
}

func OpenURI(ctx context.Context, uri string) error {
	return runDesktopCommand(ctx, "xdg-open", []string{uri}, "")
}

func ShowNotification(ctx context.Context, summary string, body ...string) error {
	args := []string{summary}
	if len(body) > 0 {
		args = append(args, body[0])
	}
	return runDesktopCommand(ctx, "notify-send", args, "")
}

func RunCommand(ctx context.Context, command []string) (CommandResult, error) {
	return runCommandWithStdin(ctx, command, "")
}

func runCommandWithStdin(ctx context.Context, command []string, stdin string) (CommandResult, error) {
	if len(command) == 0 {
		return CommandResult{}, fmt.Errorf("command must not be empty")
	}
	cmd := exec.CommandContext(ctx, command[0], command[1:]...)
	if stdin != "" {
		cmd.Stdin = strings.NewReader(stdin)
	}
	var stdout bytes.Buffer
	var stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	err := cmd.Run()
	result := CommandResult{
		Stdout: stdout.String(),
		Stderr: stderr.String(),
		RC:     0,
	}
	if cmd.ProcessState != nil {
		result.RC = cmd.ProcessState.ExitCode()
	}
	if err != nil {
		if _, ok := err.(*exec.ExitError); ok {
			return result, nil
		}
		return result, err
	}
	return result, nil
}

type treePayload struct {
	Root Widget `json:"root"`
}

type Runtime[S any] struct {
	applet Applet[S]
	reader io.Reader
	writer io.Writer
	mu     sync.Mutex

	lastStatus     []StatusItem
	lastTree       *treePayload
	popoverOpen    bool
	inlineHandlers map[string]inlineHandler
}

func NewRuntime[S any](applet Applet[S], reader io.Reader, writer io.Writer) *Runtime[S] {
	return &Runtime[S]{applet: applet, reader: reader, writer: writer}
}

func Run[S any](ctx context.Context, applet Applet[S]) error {
	return NewRuntime(applet, os.Stdin, os.Stdout).Run(ctx)
}

func (r *Runtime[S]) Run(ctx context.Context) error {
	if err := r.applet.OnStart(ctx); err != nil {
		return err
	}
	if class := r.applet.CssClass(); class != "" {
		r.mu.Lock()
		_, err := fmt.Fprintf(r.writer, "class %s\n", class)
		r.mu.Unlock()
		if err != nil {
			return err
		}
	}
	if err := r.flush(ctx); err != nil {
		return err
	}

	eventCh := make(chan incomingMessage)
	scanErrCh := make(chan error, 1)
	go r.scanInput(ctx, eventCh, scanErrCh)

	var updates <-chan struct{}
	if notifier, ok := r.applet.(interface{ Updates() <-chan struct{} }); ok {
		updates = notifier.Updates()
	}

	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case err, ok := <-scanErrCh:
			if ok && err != nil {
				return err
			}
			scanErrCh = nil
			eventCh = nil
			return nil
		case msg, ok := <-eventCh:
			if !ok {
				eventCh = nil
				if scanErrCh == nil {
					return nil
				}
				continue
			}
			switch msg.Type {
			case "init":
				event, err := parseInitEvent(msg.Data)
				if err != nil {
					fmt.Fprintf(os.Stderr, "glimpse-sdk: ignoring malformed init: %v\n", err)
					continue
				}
				if err := r.applet.OnInit(ctx, event); err != nil {
					return err
				}
			case "event":
				event, err := parseCallbackEvent(msg.Data)
				if err != nil {
					fmt.Fprintf(os.Stderr, "glimpse-sdk: ignoring malformed event: %v\n", err)
					continue
				}
				if popoverEvent, ok := event.(PopoverEvent); ok {
					r.setPopoverOpen(popoverEvent.Open)
				}
				if err := r.dispatchCallback(ctx, event); err != nil {
					return err
				}
			default:
				continue
			}
			if err := r.flush(ctx); err != nil {
				return err
			}
		case <-updates:
			if err := r.flush(ctx); err != nil {
				return err
			}
		}
	}
}

func (r *Runtime[S]) scanInput(
	ctx context.Context,
	eventCh chan<- incomingMessage,
	errCh chan<- error,
) {
	defer close(eventCh)
	defer close(errCh)

	scanner := bufio.NewScanner(r.reader)
	for scanner.Scan() {
		line := append([]byte(nil), scanner.Bytes()...)
		if len(line) == 0 {
			continue
		}
		msg, err := parseIncomingLine(line)
		if err != nil {
			fmt.Fprintf(os.Stderr, "glimpse-sdk: ignoring malformed input: %v\n", err)
			continue
		}
		if msg.Type == "" {
			continue
		}
		select {
		case <-ctx.Done():
			return
		case eventCh <- msg:
		}
	}

	if err := scanner.Err(); err != nil {
		errCh <- err
	}
}

func collectHandlers(w Widget, out map[string]inlineHandler) {
	if w == nil {
		return
	}
	switch v := w.(type) {
	case Button:
		if v.OnClick != nil {
			fn := v.OnClick
			out["click:"+v.ID] = func(ctx context.Context, e CallbackEvent) error {
				return fn(ctx, e.(ClickEvent))
			}
		}
	case ActionItem:
		if v.OnClick != nil {
			fn := v.OnClick
			out["click:"+v.ID] = func(ctx context.Context, e CallbackEvent) error {
				return fn(ctx, e.(ClickEvent))
			}
		}
		collectHandlers(v.Left, out)
		collectHandlers(v.Right, out)
	case Switch:
		if v.OnToggle != nil {
			fn := v.OnToggle
			out["toggle:"+v.ID] = func(ctx context.Context, e CallbackEvent) error {
				return fn(ctx, e.(ToggleEvent))
			}
		}
	case ToggleButton:
		if v.OnToggle != nil {
			fn := v.OnToggle
			out["toggle:"+v.ID] = func(ctx context.Context, e CallbackEvent) error {
				return fn(ctx, e.(ToggleEvent))
			}
		}
	case Checkbox:
		if v.OnToggle != nil {
			fn := v.OnToggle
			out["toggle:"+v.ID] = func(ctx context.Context, e CallbackEvent) error {
				return fn(ctx, e.(ToggleEvent))
			}
		}
	case Slider:
		if v.OnChange != nil {
			fn := v.OnChange
			out["change:"+v.ID] = func(ctx context.Context, e CallbackEvent) error {
				return fn(ctx, e.(ChangeEvent))
			}
		}
	case Select:
		if v.OnChange != nil {
			fn := v.OnChange
			out["change:"+v.ID] = func(ctx context.Context, e CallbackEvent) error {
				return fn(ctx, e.(ChangeEvent))
			}
		}
	case Hero:
		if v.OnToggle != nil && v.ID != "" {
			fn := v.OnToggle
			id := v.ID
			out["toggle:"+id] = func(ctx context.Context, e CallbackEvent) error {
				return fn(ctx, e.(ToggleEvent))
			}
		}
	case Row:
		for _, child := range v.Children {
			collectHandlers(child, out)
		}
	case Column:
		for _, child := range v.Children {
			collectHandlers(child, out)
		}
	case Grid:
		for _, gc := range v.Children {
			collectHandlers(gc.Child, out)
		}
	case Card:
		collectHandlers(v.Child, out)
	case Container:
		collectHandlers(v.Child, out)
	case Scroll:
		collectHandlers(v.Child, out)
	case Expander:
		collectHandlers(v.Child, out)
	case PopoverScaffold:
		collectHandlers(v.Hero, out)
		collectHandlers(v.Body, out)
	case Item:
		collectHandlers(v.Left, out)
		collectHandlers(v.Right, out)
	}
}

func (r *Runtime[S]) flush(ctx context.Context) error {
	state := r.applet.State()

	statusItems, err := r.applet.Status(ctx, state)
	if err != nil {
		return err
	}
	if !statusEqual(r.lastStatus, statusItems) {
		if err := r.writeMessage("status", map[string]any{"items": statusItems}); err != nil {
			return err
		}
		r.lastStatus = append([]StatusItem(nil), statusItems...)
	}

	widget, err := r.applet.Popover(ctx, state)
	if err != nil {
		return err
	}
	r.inlineHandlers = make(map[string]inlineHandler)
	if widget != nil {
		collectHandlers(widget, r.inlineHandlers)
	}
	tree := &treePayload{Root: widget}
	if !treePayloadEqual(r.lastTree, tree) {
		if err := r.writeMessage("popover", tree); err != nil {
			return err
		}
		r.lastTree = tree
	}
	return nil
}

func (r *Runtime[S]) dispatchCallback(ctx context.Context, event CallbackEvent) error {
	var key string
	switch e := event.(type) {
	case ClickEvent:
		key = "click:" + e.ID
	case ToggleEvent:
		key = "toggle:" + e.ID
	case ChangeEvent:
		key = "change:" + e.ID
	}
	if key != "" {
		if h, ok := r.inlineHandlers[key]; ok {
			return h(ctx, event)
		}
	}

	switch e := event.(type) {
	case ClickEvent:
		if h, ok := r.applet.(ClickHandler); ok {
			return h.OnClick(ctx, e)
		}
	case ScrollEvent:
		if h, ok := r.applet.(ScrollHandler); ok {
			return h.OnScroll(ctx, e)
		}
	case InputEvent:
		if h, ok := r.applet.(InputHandler); ok {
			return h.OnInput(ctx, e)
		}
	case ChangeEvent:
		if h, ok := r.applet.(ChangeHandler); ok {
			return h.OnChange(ctx, e)
		}
	case ToggleEvent:
		if h, ok := r.applet.(ToggleHandler); ok {
			return h.OnToggle(ctx, e)
		}
	case PopoverEvent:
		if h, ok := r.applet.(PopoverHandler); ok {
			return h.OnPopover(ctx, e)
		}
	}
	return r.applet.OnCallback(ctx, event)
}

func (r *Runtime[S]) setPopoverOpen(open bool) {
	r.popoverOpen = open
	if stateful, ok := r.applet.(interface{ SetPopoverOpen(bool) }); ok {
		stateful.SetPopoverOpen(open)
	}
}

func (r *Runtime[S]) writeMessage(kind string, data any) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	encoded, err := json.Marshal(data)
	if err != nil {
		return err
	}
	if _, err := fmt.Fprintf(r.writer, "%s %s\n", kind, encoded); err != nil {
		return err
	}
	return nil
}

func statusEqual(left, right []StatusItem) bool {
	encodedLeft, _ := json.Marshal(left)
	encodedRight, _ := json.Marshal(right)
	return string(encodedLeft) == string(encodedRight)
}

func treePayloadEqual(left, right *treePayload) bool {
	encodedLeft, _ := json.Marshal(left)
	encodedRight, _ := json.Marshal(right)
	return string(encodedLeft) == string(encodedRight)
}
