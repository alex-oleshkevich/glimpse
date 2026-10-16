package sdk

import (
	"bufio"
	"encoding/json"
	"net"
	"path/filepath"
	"testing"
	"time"
)

type pct float64

func TestCleanNamedFloat(t *testing.T) {
	if clean(pct(nanValue())) != nil {
		t.Error("named NaN float must clean to nil")
	}
	if clean(pct(1.5)) != pct(1.5) {
		t.Error("finite named float must pass through")
	}
}

func nanValue() float64 { z := 0.0; return z / z }

type runApp struct{ build func() []Widget }

func (a runApp) Build(*Context) []Widget { return a.build() }

func runOverSocket(t *testing.T, app App, send func(c *net.UnixConn)) (int, []map[string]any) {
	t.Helper()
	path := filepath.Join(t.TempDir(), "s.sock")
	ln, err := net.Listen("unix", path)
	if err != nil {
		t.Fatal(err)
	}
	defer ln.Close()
	t.Setenv("GLIMPSE_APPLET_SOCKET", path)

	done := make(chan int, 1)
	go func() { done <- run(app) }()

	conn, err := ln.Accept()
	if err != nil {
		t.Fatal(err)
	}
	defer conn.Close()
	send(conn.(*net.UnixConn))

	var got []map[string]any
	sc := bufio.NewScanner(conn)
	for sc.Scan() {
		var m map[string]any
		if err := json.Unmarshal(sc.Bytes(), &m); err != nil {
			t.Fatal(err)
		}
		got = append(got, m)
	}
	select {
	case code := <-done:
		return code, got
	case <-time.After(5 * time.Second):
		t.Fatal("run did not exit")
		return 0, nil
	}
}

func writeMsg(t *testing.T, c net.Conn, msg map[string]any) {
	t.Helper()
	b, _ := json.Marshal(msg)
	if _, err := c.Write(append(b, '\n')); err != nil {
		t.Fatal(err)
	}
}

func TestRunFlushesBatchAtEOF(t *testing.T) {
	app := runApp{func() []Widget { return []Widget{Indicator(IndicatorProps{Icon: "x"}, "hi")} }}
	code, got := runOverSocket(t, app, func(c *net.UnixConn) {
		writeMsg(t, c, hello)
		c.CloseWrite()
	})
	if code != 0 {
		t.Fatalf("exit code %d", code)
	}
	if len(got) != 2 || got[0]["t"] != "hello" || got[1]["t"] != "commit" {
		t.Fatalf("want hello then commit, got %v", got)
	}
}

func TestRunExitCodes(t *testing.T) {
	over := runApp{func() []Widget {
		kids := make([]any, maxChildren+1)
		for i := range kids {
			kids[i] = Separator(SeparatorProps{})
		}
		return []Widget{Popover(PopoverProps{}, kids)}
	}}
	boom := runApp{func() []Widget { panic("boom") }}
	for name, c := range map[string]struct {
		app  App
		want int
	}{"limit": {over, 70}, "build panic": {boom, 70}} {
		code, _ := runOverSocket(t, c.app, func(conn *net.UnixConn) {
			writeMsg(t, conn, hello)
			conn.CloseWrite()
		})
		if code != c.want {
			t.Errorf("%s: exit %d, want %d", name, code, c.want)
		}
	}
}

func TestRunMissingSocketExitsZero(t *testing.T) {
	t.Setenv("GLIMPSE_APPLET_SOCKET", filepath.Join(t.TempDir(), "none"))
	if code := run(runApp{func() []Widget { return nil }}); code != 0 {
		t.Fatalf("exit %d", code)
	}
}
