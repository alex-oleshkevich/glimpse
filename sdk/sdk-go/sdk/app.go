package sdk

import (
	"bufio"
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

type treePayload struct {
	Root Widget `json:"root"`
}

type Runtime[S any] struct {
	applet Applet[S]
	reader io.Reader
	writer io.Writer
	mu     sync.Mutex

	lastStatus  []StatusItem
	lastTree    *treePayload
	popoverOpen bool
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
				if err := r.applet.OnCallback(ctx, event); err != nil {
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
	tree := &treePayload{Root: widget}
	if !treePayloadEqual(r.lastTree, tree) {
		if err := r.writeMessage("popover", tree); err != nil {
			return err
		}
		r.lastTree = tree
	}
	return nil
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
