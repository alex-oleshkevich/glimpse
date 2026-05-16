package sdk

// Minimal client for the Glimpse IPC socket.
//
// IPC(service) resolves a *Subscriber (no I/O — the connection is opened
// lazily). Subscriber.Listen subscribes to an event channel and streams
// decoded Events over a channel; Subscriber.Dispatch sends an action and
// awaits the server ack on a one-shot connection. The wire protocol matches
// the `glimpse-shell watch` / `dispatch` CLIs.

import (
	"bufio"
	"context"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync"
)

// Event is one decoded event line; Fields values are unescaped.
type Event struct {
	Name   string
	Ts     int64
	Fields map[string]string
}

// Subscriber is a resolved IPC endpoint. Cheap to create; holds only the
// socket path (or the error from resolving it, surfaced on first use).
type Subscriber struct {
	socket     string
	resolveErr error

	mu  sync.Mutex
	err error
}

// IPC resolves the Subscriber for service (use "shell", or "" for it, to
// reach the panel). The socket is <dir>/<service>.sock — "shell" maps to
// ipc.sock — where <dir> is $GLIMPSE_IPC_DIR, else $XDG_RUNTIME_DIR/glimpse.
// No connection is made here; resolution errors surface from Listen/Dispatch.
func IPC(service string) *Subscriber {
	socket, err := socketPath(service)
	return &Subscriber{socket: socket, resolveErr: err}
}

func socketPath(service string) (string, error) {
	if service == "" {
		service = "shell"
	}
	var dir string
	if v := os.Getenv("GLIMPSE_IPC_DIR"); v != "" {
		dir = v
	} else if x := os.Getenv("XDG_RUNTIME_DIR"); x != "" {
		dir = filepath.Join(x, "glimpse")
	} else {
		return "", fmt.Errorf(
			"neither GLIMPSE_IPC_DIR nor XDG_RUNTIME_DIR is set; " +
				"cannot locate the Glimpse IPC socket")
	}
	name := "ipc.sock"
	if service != "shell" {
		name = service + ".sock"
	}
	return filepath.Join(dir, name), nil
}

// Err returns why a Listen stream ended (nil for a clean EOF or a canceled
// context). Read it after the Listen channel closes.
func (s *Subscriber) Err() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.err
}

func (s *Subscriber) setErr(err error) {
	s.mu.Lock()
	s.err = err
	s.mu.Unlock()
}

// Listen subscribes to channel (an exact name, a "prefix.*" pattern, or "*")
// and streams events until the server closes the connection or ctx is
// canceled. The returned channel is closed when the stream ends; inspect
// Err afterwards for the terminal cause.
//
// The caller MUST cancel ctx when done (e.g. break out of the range loop and
// cancel): a reader goroutine and the socket fd stay alive until the server
// closes the connection or ctx is canceled, so abandoning the channel
// without canceling leaks both.
func (s *Subscriber) Listen(ctx context.Context, channel string) (<-chan Event, error) {
	conn, br, err := s.connect(ctx)
	if err != nil {
		return nil, err
	}
	if _, err := io.WriteString(conn, "subscribe "+channel+"\n"); err != nil {
		conn.Close()
		return nil, err
	}
	ch := make(chan Event)
	stop := context.AfterFunc(ctx, func() { conn.Close() })
	go func() {
		defer close(ch)
		defer stop()
		defer conn.Close()
		for {
			line, err := br.ReadString('\n')
			if err != nil {
				if ctx.Err() == nil && err != io.EOF {
					s.setErr(err)
				}
				return
			}
			line = strings.TrimSpace(line)
			if line == "" {
				continue
			}
			select {
			case ch <- parseEvent(line):
			case <-ctx.Done():
				return
			}
		}
	}()
	return ch, nil
}

// Dispatch sends action with params on a fresh connection and awaits the
// ack. It returns the extra ack fields, or an error if connecting fails or
// the server replies ok=false.
func (s *Subscriber) Dispatch(
	ctx context.Context, action string, params map[string]string,
) (map[string]string, error) {
	if err := validateToken("action", action, false); err != nil {
		return nil, err
	}
	keys := make([]string, 0, len(params))
	for k := range params {
		if err := validateToken("param key", k, true); err != nil {
			return nil, err
		}
		keys = append(keys, k)
	}
	// Deterministic wire order (Go map iteration is randomized).
	sort.Strings(keys)

	conn, br, err := s.connect(ctx)
	if err != nil {
		return nil, err
	}
	defer conn.Close()
	stop := context.AfterFunc(ctx, func() { conn.Close() })
	defer stop()

	var b strings.Builder
	b.WriteString(action)
	for _, k := range keys {
		b.WriteByte(' ')
		b.WriteString(k)
		b.WriteByte('=')
		b.WriteString(escapeValue(params[k]))
	}
	b.WriteByte('\n')
	if _, err := io.WriteString(conn, b.String()); err != nil {
		return nil, err
	}
	line, err := br.ReadString('\n')
	if err != nil {
		return nil, fmt.Errorf("reading IPC ack: %w", err)
	}
	return parseAck(strings.TrimSpace(line))
}

func (s *Subscriber) connect(ctx context.Context) (net.Conn, *bufio.Reader, error) {
	if s.resolveErr != nil {
		return nil, nil, s.resolveErr
	}
	var d net.Dialer
	conn, err := d.DialContext(ctx, "unix", s.socket)
	if err != nil {
		return nil, nil, fmt.Errorf(
			"cannot connect to IPC socket at %s: %w", s.socket, err)
	}
	br := bufio.NewReader(conn)
	hello, err := br.ReadString('\n')
	if err != nil {
		conn.Close()
		return nil, nil, fmt.Errorf("IPC server closed connection before hello: %w", err)
	}
	if !strings.HasPrefix(hello, "hello") {
		conn.Close()
		return nil, nil, fmt.Errorf(
			"unexpected IPC greeting: %s", strings.TrimSpace(hello))
	}
	return conn, br, nil
}

// validateToken rejects action names / field keys that would forge extra
// tokens or whole client lines: the wire protocol splits on whitespace and
// never unescapes the command name or a key (only values are escaped).
func validateToken(label, token string, forbidEq bool) error {
	if token == "" {
		return fmt.Errorf("IPC %s must not be empty", label)
	}
	if strings.ContainsAny(token, " \t\n\r\f\v") {
		return fmt.Errorf("IPC %s %q must not contain whitespace", label, token)
	}
	if forbidEq && strings.Contains(token, "=") {
		return fmt.Errorf("IPC param key %q must not contain '='", token)
	}
	return nil
}

func escapeValue(s string) string {
	r := strings.NewReplacer(
		"\\", "\\\\",
		"\n", "\\n",
		"\t", "\\t",
		" ", "\\s",
	)
	return r.Replace(s)
}

func unescapeValue(s string) string {
	var b strings.Builder
	b.Grow(len(s))
	for i := 0; i < len(s); i++ {
		if s[i] != '\\' {
			b.WriteByte(s[i])
			continue
		}
		i++
		if i >= len(s) {
			b.WriteByte('\\')
			break
		}
		switch s[i] {
		case 's':
			b.WriteByte(' ')
		case 'n':
			b.WriteByte('\n')
		case 't':
			b.WriteByte('\t')
		case '\\':
			b.WriteByte('\\')
		default:
			b.WriteByte('\\')
			b.WriteByte(s[i])
		}
	}
	return b.String()
}

func parseEvent(line string) Event {
	tokens := strings.Fields(line)
	ev := Event{Fields: map[string]string{}}
	if len(tokens) > 0 {
		ev.Name = tokens[0]
	}
	for _, token := range tokens[1:] {
		key, raw, ok := strings.Cut(token, "=")
		if !ok {
			continue
		}
		value := unescapeValue(raw)
		if key == "ts" {
			if n, err := strconv.ParseInt(value, 10, 64); err == nil {
				ev.Ts = n
				continue
			}
		}
		ev.Fields[key] = value
	}
	return ev
}

func parseAck(line string) (map[string]string, error) {
	tokens := strings.Fields(line)
	if len(tokens) == 0 || tokens[0] != "ack" {
		return nil, fmt.Errorf("expected an ack, got: %s", line)
	}
	ok := false
	fields := map[string]string{}
	for _, token := range tokens[1:] {
		key, raw, has := strings.Cut(token, "=")
		if !has {
			continue
		}
		value := unescapeValue(raw)
		if key == "ok" {
			ok = value == "true"
		} else {
			fields[key] = value
		}
	}
	if !ok {
		if msg, has := fields["error"]; has {
			return nil, fmt.Errorf("%s", msg)
		}
		return nil, fmt.Errorf("command failed")
	}
	return fields, nil
}
