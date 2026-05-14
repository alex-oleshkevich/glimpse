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
		{ID: "demo", Icon: IconName("demo-symbolic"), Label: state.Version},
	}, nil
}

func (a *demoApplet) Popover(_ context.Context, state *demoState) (Widget, error) {
	if !state.Tree {
		return nil, nil
	}
	return Column{
		Children: []Widget{
			Hero{Title: "Demo", Subtitle: state.Version},
			Label{Text: state.Version},
			Button{CommonProps: CommonProps{ID: "submit"}, Label: "Submit"},
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

func TestSelectSerializesItems(t *testing.T) {
	widget := Select{
		CommonProps: CommonProps{ID: "env"},
		Items:       []SelectOption{{ID: "prod", Label: "Production"}},
	}
	payload, err := json.Marshal(widget)
	if err != nil {
		t.Fatalf("marshal select: %v", err)
	}
	var decoded map[string]any
	if err := json.Unmarshal(payload, &decoded); err != nil {
		t.Fatalf("unmarshal select: %v", err)
	}
	if decoded["type"] != "select" {
		t.Fatalf("expected select type, got %v", decoded["type"])
	}
}

func TestVariantSerializesAsSemanticProtocolValue(t *testing.T) {
	widget := Label{CommonProps: CommonProps{Variant: VariantWarning}, Text: "Warning"}
	payload, err := json.Marshal(widget)
	if err != nil {
		t.Fatalf("marshal label: %v", err)
	}
	if !strings.Contains(string(payload), `"variant":"warning"`) {
		t.Fatalf("expected warning variant, got %s", payload)
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

func TestRuntimeFlushesActionHelperMessages(t *testing.T) {
	applet := newDemoApplet()
	var output bytes.Buffer
	runtime := NewRuntime[demoState](applet, bytes.NewBufferString(""), &output)

	applet.ShowNotification(ShowNotificationArgs{
		Summary: "Backup finished",
		Body:    "42 files synced",
		Urgency: NotificationUrgencyNormal,
	})
	applet.OpenURI(OpenURIArgs{URI: "https://example.com/docs"})
	applet.CopyToClipboard(CopyToClipboardArgs{Text: "device-42"})
	applet.DismissNotification(DismissNotificationArgs{ID: 42})
	applet.ClosePopover()

	if err := runtime.flushActions(); err != nil {
		t.Fatalf("flush actions: %v", err)
	}

	lines := strings.Split(strings.TrimSpace(output.String()), "\n")
	expected := []string{
		`action {"type":"show_notification","arguments":{"summary":"Backup finished","body":"42 files synced","urgency":"normal"}}`,
		`action {"type":"open_uri","arguments":{"uri":"https://example.com/docs"}}`,
		`action {"type":"copy_to_clipboard","arguments":{"text":"device-42"}}`,
		`action {"type":"dismiss_notification","arguments":{"id":42}}`,
		`action {"type":"close_popover","arguments":{}}`,
	}
	if !reflect.DeepEqual(lines, expected) {
		t.Fatalf("unexpected action lines:\nwant %#v\ngot  %#v", expected, lines)
	}
}

func TestRuntimeDropsClosedPopoverUpdatesAfterInitialTree(t *testing.T) {
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
		t.Fatalf("closed flush render: %v", err)
	}
	if strings.Contains(output.String(), "popover ") {
		t.Fatalf("expected closed popover update to be dropped, got %q", output.String())
	}
	if !strings.Contains(output.String(), "status ") {
		t.Fatalf("expected status updates to continue while popover is closed, got %q", output.String())
	}

	output.Reset()
	runtime.setPopoverOpen(true)
	if err := runtime.flush(context.Background()); err != nil {
		t.Fatalf("open flush render: %v", err)
	}
	if !strings.Contains(output.String(), "popover ") {
		t.Fatalf("expected fresh popover update after open, got %q", output.String())
	}
	if !strings.Contains(output.String(), "v2") {
		t.Fatalf("expected latest popover model after open, got %q", output.String())
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
		{ID: "demo", Icon: IconName("demo-symbolic"), Label: state.Version},
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
