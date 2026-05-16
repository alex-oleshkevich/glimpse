package sdk

import (
	"bufio"
	"context"
	"net"
	"path/filepath"
	"strings"
	"testing"
)

func TestEscapeRoundtrip(t *testing.T) {
	s := "a b\tc\nd\\e"
	if got := unescapeValue(escapeValue(s)); got != s {
		t.Fatalf("roundtrip: got %q want %q", got, s)
	}
}

func TestParseEvent(t *testing.T) {
	ev := parseEvent("notification.received body=l1\\nl2\\sword ts=42")
	if ev.Name != "notification.received" {
		t.Fatalf("name = %q", ev.Name)
	}
	if ev.Ts != 42 {
		t.Fatalf("ts = %d", ev.Ts)
	}
	if ev.Fields["body"] != "l1\nl2 word" {
		t.Fatalf("body = %q", ev.Fields["body"])
	}
}

func TestParseAckFailure(t *testing.T) {
	if _, err := parseAck("ack ok=false error=nope"); err == nil {
		t.Fatal("expected error for ok=false")
	}
	fields, err := parseAck("ack ok=true echo=hi")
	if err != nil || fields["echo"] != "hi" {
		t.Fatalf("ok ack: fields=%v err=%v", fields, err)
	}
}

func TestDispatchAndListenAgainstFakeServer(t *testing.T) {
	dir := t.TempDir()
	socket := filepath.Join(dir, "ipc.sock")
	ln, err := net.Listen("unix", socket)
	if err != nil {
		t.Fatalf("listen: %v", err)
	}
	defer ln.Close()

	go func() {
		for {
			conn, err := ln.Accept()
			if err != nil {
				return
			}
			go func(c net.Conn) {
				defer c.Close()
				rw := bufio.NewReadWriter(
					bufio.NewReader(c), bufio.NewWriter(c))
				_, _ = rw.WriteString("hello version=test\n")
				_ = rw.Flush()
				line, _ := rw.ReadString('\n')
				switch {
				case len(line) >= 10 && line[:10] == "subscribe ":
					_, _ = rw.WriteString(
						"audio.volume_changed volume=42 ts=7\n")
				default:
					_, _ = rw.WriteString("ack ok=true echo=done\n")
				}
				_ = rw.Flush()
			}(conn)
		}
	}()

	sub := &Subscriber{socket: socket}
	ctx := context.Background()

	ack, err := sub.Dispatch(ctx, "open_uri", map[string]string{
		"uri": "https://example.com",
	})
	if err != nil {
		t.Fatalf("dispatch: %v", err)
	}
	if ack["echo"] != "done" {
		t.Fatalf("ack = %v", ack)
	}

	lctx, cancel := context.WithCancel(ctx)
	defer cancel()
	events, err := sub.Listen(lctx, "audio.*")
	if err != nil {
		t.Fatalf("listen: %v", err)
	}
	ev := <-events
	if ev.Name != "audio.volume_changed" || ev.Ts != 7 ||
		ev.Fields["volume"] != "42" {
		t.Fatalf("event = %+v", ev)
	}
}

func TestValidateTokenRejectsInjection(t *testing.T) {
	if err := validateToken("action", "open_uri", false); err != nil {
		t.Fatalf("valid action rejected: %v", err)
	}
	for _, bad := range []string{"a\nsubscribe *", "a b", ""} {
		if err := validateToken("action", bad, false); err == nil {
			t.Fatalf("expected rejection for %q", bad)
		}
	}
	if err := validateToken("param key", "k=v", true); err == nil {
		t.Fatal("expected rejection for key containing '='")
	}
}

func TestDispatchRejectsUnsafeActionBeforeConnect(t *testing.T) {
	sub := &Subscriber{socket: "/nonexistent/glimpse-x.sock"}
	_, err := sub.Dispatch(context.Background(), "evil\naction", nil)
	if err == nil || !strings.Contains(err.Error(), "whitespace") {
		t.Fatalf("expected whitespace rejection, got %v", err)
	}
	_, err = sub.Dispatch(context.Background(), "ok",
		map[string]string{"bad key": "v"})
	if err == nil || !strings.Contains(err.Error(), "whitespace") {
		t.Fatalf("expected key rejection before connect, got %v", err)
	}
}

func TestConnectFailureIsError(t *testing.T) {
	sub := &Subscriber{socket: "/nonexistent/glimpse-missing.sock"}
	if _, err := sub.Dispatch(context.Background(), "noop", nil); err == nil {
		t.Fatal("expected connect error")
	}
}

func TestIPCResolvesSocketPath(t *testing.T) {
	t.Setenv("GLIMPSE_IPC_DIR", "/run/glimpse-test")
	if got := IPC("shell").socket; got != "/run/glimpse-test/ipc.sock" {
		t.Fatalf("shell socket = %q", got)
	}
	if got := IPC("idle").socket; got != "/run/glimpse-test/idle.sock" {
		t.Fatalf("idle socket = %q", got)
	}
	if got := IPC("").socket; got != "/run/glimpse-test/ipc.sock" {
		t.Fatalf("default socket = %q", got)
	}
}
