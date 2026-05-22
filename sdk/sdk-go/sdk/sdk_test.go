package sdk

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"strings"
	"testing"
	"time"
)

type demoState struct {
	Version string
	Clicks  int
	Tree    bool
}

type demoApplet struct {
	BaseApplet[demoState]
}

func newDemoApplet() *demoApplet {
	return &demoApplet{
		BaseApplet: NewBaseApplet(demoState{Version: "v1", Tree: true}),
	}
}

func (a *demoApplet) OnStart(context.Context) error           { return nil }
func (a *demoApplet) OnInit(context.Context, InitEvent) error { return nil }

func (a *demoApplet) OnCallback(_ context.Context, event CallbackEvent) error {
	switch e := event.(type) {
	case ClickEvent:
		if e.ID == "submit" {
			a.SetState(func(state *demoState) {
				state.Clicks++
				state.Version = "v2"
			})
		}
	}
	return nil
}

func (a *demoApplet) Status(_ context.Context, state *demoState) ([]StatusItem, error) {
	return []StatusItem{
		{ID: "demo", Icon: "demo-symbolic", Label: state.Version},
	}, nil
}

func (a *demoApplet) Popover(_ context.Context, state *demoState) (Widget, error) {
	if !state.Tree {
		return nil, nil
	}
	return Column{
		Children: []Widget{
			Hero{Title: "Demo", Subtitle: state.Version},
			Text{Text: state.Version},
			Tile{ID: "submit", Primary: "Submit", Activatable: true},
		},
	}, nil
}

func TestParseCallbackEventReturnsTypedClickVariant(t *testing.T) {
	event, err := parseCallbackEvent([]byte(`{"id":"submit","type":"click","button":"left"}`))
	if err != nil {
		t.Fatalf("parse callback event: %v", err)
	}
	click, ok := event.(ClickEvent)
	if !ok {
		t.Fatalf("expected ClickEvent, got %T", event)
	}
	if click.Button != "left" {
		t.Fatalf("expected left button, got %q", click.Button)
	}
}

func TestParseCallbackEventReturnsTypedPopoverVariant(t *testing.T) {
	event, err := parseCallbackEvent([]byte(`{"id":"popover","type":"open","source":"popover"}`))
	if err != nil {
		t.Fatalf("parse callback event: %v", err)
	}
	popover, ok := event.(PopoverEvent)
	if !ok {
		t.Fatalf("expected PopoverEvent, got %T", event)
	}
	if !popover.Open {
		t.Fatal("expected open popover event")
	}
}

func TestChoiceListSerializesChoices(t *testing.T) {
	widget := ChoiceList{
		ID:      "env",
		Choices: []Choice{{ID: "prod", Primary: "Production"}},
	}
	payload, err := json.Marshal(widget)
	if err != nil {
		t.Fatalf("marshal choice list: %v", err)
	}
	var decoded map[string]any
	if err := json.Unmarshal(payload, &decoded); err != nil {
		t.Fatalf("unmarshal choice list: %v", err)
	}
	if decoded["type"] != "choice_list" {
		t.Fatalf("expected choice_list type, got %v", decoded["type"])
	}
}

func TestVariantSerializesAsSemanticProtocolValue(t *testing.T) {
	widget := Badge{Label: "Warning", Kind: BadgeKindWarning}
	payload, err := json.Marshal(widget)
	if err != nil {
		t.Fatalf("marshal badge: %v", err)
	}
	if !strings.Contains(string(payload), `"kind":"warning"`) {
		t.Fatalf("expected warning kind, got %s", payload)
	}
}

func TestRuntimeFlushesRenderedMessages(t *testing.T) {
	applet := newDemoApplet()
	var output bytes.Buffer
	runtime := NewRuntime[demoState](applet, bytes.NewBufferString(""), &output)

	if err := runtime.flush(context.Background()); err != nil {
		t.Fatalf("flush render: %v", err)
	}

	lines := bytes.Split(bytes.TrimSpace(output.Bytes()), []byte("\n"))
	if len(lines) != 2 {
		t.Fatalf("expected 2 messages, got %d", len(lines))
	}
}

func TestRuntimeEmitsPopoverUpdatesWhenStateChanges(t *testing.T) {
	applet := newDemoApplet()
	var output bytes.Buffer
	runtime := NewRuntime[demoState](applet, bytes.NewBufferString(""), &output)

	if err := runtime.flush(context.Background()); err != nil {
		t.Fatalf("initial flush render: %v", err)
	}
	output.Reset()

	applet.SetState(func(state *demoState) {
		state.Version = "v2"
	})
	if err := runtime.flush(context.Background()); err != nil {
		t.Fatalf("flush after state change: %v", err)
	}
	if !strings.Contains(output.String(), "status ") {
		t.Fatalf("expected status update, got %q", output.String())
	}
	if !strings.Contains(output.String(), "popover ") {
		t.Fatalf("expected popover update even while closed, got %q", output.String())
	}
	if !strings.Contains(output.String(), "v2") {
		t.Fatalf("expected latest popover model, got %q", output.String())
	}
}

func TestRuntimePublishesClosedPopoverRemoval(t *testing.T) {
	applet := newDemoApplet()
	var output bytes.Buffer
	runtime := NewRuntime[demoState](applet, bytes.NewBufferString(""), &output)

	if err := runtime.flush(context.Background()); err != nil {
		t.Fatalf("initial flush render: %v", err)
	}
	output.Reset()

	applet.SetState(func(state *demoState) {
		state.Tree = false
	})
	if err := runtime.flush(context.Background()); err != nil {
		t.Fatalf("closed removal flush render: %v", err)
	}
	if !strings.Contains(output.String(), "popover ") {
		t.Fatalf("expected popover removal to publish while closed, got %q", output.String())
	}
	if !strings.Contains(output.String(), `"root":null`) {
		t.Fatalf("expected nil popover root removal, got %q", output.String())
	}
}

func TestRuntimeExposesPopoverOpenBeforeCallback(t *testing.T) {
	applet := newDemoApplet()
	runtime := NewRuntime[demoState](applet, bytes.NewBufferString(""), io.Discard)

	runtime.setPopoverOpen(true)

	if !applet.IsPopoverOpen() {
		t.Fatal("expected applet to observe open popover state")
	}
}

func TestSetStateUpdatesRenderedStatus(t *testing.T) {
	applet := newDemoApplet()
	if err := applet.OnCallback(context.Background(), ClickEvent{ID: "submit", Button: "left"}); err != nil {
		t.Fatalf("callback: %v", err)
	}
	status, err := applet.Status(context.Background(), applet.State())
	if err != nil {
		t.Fatalf("status: %v", err)
	}
	if status[0].Label != "v2" {
		t.Fatalf("expected updated status label, got %q", status[0].Label)
	}
}

func TestDesktopHelpersRunLocalCommands(t *testing.T) {
	var calls []struct {
		command string
		args    []string
		stdin   string
	}
	original := runDesktopCommand
	runDesktopCommand = func(ctx context.Context, command string, args []string, stdin string) error {
		calls = append(calls, struct {
			command string
			args    []string
			stdin   string
		}{command: command, args: append([]string(nil), args...), stdin: stdin})
		return nil
	}
	t.Cleanup(func() {
		runDesktopCommand = original
	})

	ctx := context.Background()
	if err := CopyToClipboard(ctx, "hello"); err != nil {
		t.Fatalf("copy to clipboard: %v", err)
	}
	if err := OpenURI(ctx, "https://example.com"); err != nil {
		t.Fatalf("open URI: %v", err)
	}
	if err := ShowNotification(ctx, "Build complete", "Tests passed"); err != nil {
		t.Fatalf("show notification: %v", err)
	}

	if len(calls) != 3 {
		t.Fatalf("expected 3 command calls, got %d", len(calls))
	}
	if calls[0].command != "wl-copy" || len(calls[0].args) != 0 {
		t.Fatalf("unexpected clipboard command: %#v", calls[0])
	}
	if calls[0].stdin != "hello" {
		t.Fatalf("unexpected clipboard stdin: %q", calls[0].stdin)
	}
	if calls[1].command != "xdg-open" || strings.Join(calls[1].args, " ") != "https://example.com" {
		t.Fatalf("unexpected open URI command: %#v", calls[1])
	}
	if calls[2].command != "notify-send" || strings.Join(calls[2].args, " ") != "Build complete Tests passed" {
		t.Fatalf("unexpected notification command: %#v", calls[2])
	}
}

func TestRunCommandReturnsStdoutStderrAndRC(t *testing.T) {
	result, err := RunCommand(context.Background(), []string{
		"sh",
		"-c",
		"printf 'out\\n'; printf 'err\\n' >&2; exit 7",
	})
	if err != nil {
		t.Fatalf("run command: %v", err)
	}
	if result.Stdout != "out\n" {
		t.Fatalf("unexpected stdout: %q", result.Stdout)
	}
	if result.Stderr != "err\n" {
		t.Fatalf("unexpected stderr: %q", result.Stderr)
	}
	if result.RC != 7 {
		t.Fatalf("unexpected rc: %d", result.RC)
	}
}

func TestRunCommandRejectsEmptyCommand(t *testing.T) {
	_, err := RunCommand(context.Background(), nil)
	if err == nil {
		t.Fatal("expected empty command to fail")
	}
}

func ptr[T any](value T) *T {
	return &value
}

type asyncDemoApplet struct {
	BaseApplet[demoState]
}

func newAsyncDemoApplet() *asyncDemoApplet {
	return &asyncDemoApplet{
		BaseApplet: NewBaseApplet(demoState{Version: "v1"}),
	}
}

func (a *asyncDemoApplet) OnStart(context.Context) error {
	go func() {
		time.Sleep(20 * time.Millisecond)
		a.SetState(func(state *demoState) {
			state.Version = "v2"
		})
	}()
	return nil
}

func (a *asyncDemoApplet) OnInit(context.Context, InitEvent) error         { return nil }
func (a *asyncDemoApplet) OnCallback(context.Context, CallbackEvent) error { return nil }

func (a *asyncDemoApplet) Status(_ context.Context, state *demoState) ([]StatusItem, error) {
	return []StatusItem{
		{ID: "demo", Icon: "demo-symbolic", Label: state.Version},
	}, nil
}

func (a *asyncDemoApplet) Popover(_ context.Context, _ *demoState) (Widget, error) {
	return nil, nil
}

func TestRuntimeFlushesWhenStateChangesWithoutInput(t *testing.T) {
	inputReader, inputWriter := io.Pipe()
	defer inputWriter.Close()
	outputReader, outputWriter := io.Pipe()
	defer outputReader.Close()

	runtime := NewRuntime[demoState](newAsyncDemoApplet(), inputReader, outputWriter)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	done := make(chan error, 1)
	go func() {
		done <- runtime.Run(ctx)
	}()

	scanner := bufio.NewScanner(outputReader)
	var sawV1 bool
	var sawV2 bool
	deadline := time.After(500 * time.Millisecond)

	for !sawV2 {
		select {
		case <-deadline:
			t.Fatalf("expected async state update to flush output; sawV1=%v sawV2=%v", sawV1, sawV2)
		default:
		}

		if !scanner.Scan() {
			time.Sleep(10 * time.Millisecond)
			continue
		}
		line := scanner.Text()
		if !strings.HasPrefix(line, "status ") {
			continue
		}
		if strings.Contains(line, "\"label\":\"v1\"") {
			sawV1 = true
		}
		if strings.Contains(line, "\"label\":\"v2\"") {
			sawV2 = true
		}
	}

	cancel()
	inputWriter.Close()
	outputWriter.Close()

	err := <-done
	if err != nil && !errors.Is(err, context.Canceled) {
		t.Fatalf("runtime returned unexpected error: %v", err)
	}
	if !sawV1 {
		t.Fatal("expected initial render before async update")
	}
}
